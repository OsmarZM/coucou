// Runs the freshly built parser worker, without starting Tauri or an AI turn.
// The deadline case proves harness cleanup only. The production caller owns
// the Windows Job, memory/CPU limits, cancellation and its 30-second deadline.
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdtemp, mkdir, realpath, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const RESPONSE_LIMIT = 16 * 1024 * 1024;
const TIMEOUT_MS = 10_000;
const windowsRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const expectedExecutables = ["debug", "release"].map(profile =>
  join(windowsRoot, "target", profile, "coucou.exe"));
const normalize = (value) => process.platform === "win32" ? value.toLowerCase() : value;
const summary = (value) => console.log(JSON.stringify(value));

function crc32(bytes) {
  let value = 0xffffffff;
  for (const byte of bytes) {
    value ^= byte;
    for (let bit = 0; bit < 8; bit++) {
      value = (value >>> 1) ^ ((value & 1) ? 0xedb88320 : 0);
    }
  }
  return (value ^ 0xffffffff) >>> 0;
}

// Minimal ZIP writer using stored entries; fixtures need no installed office
// application, archive dependency or downloads.
function zip(entries) {
  const local = [];
  const central = [];
  let offset = 0;
  for (const [entryName, value] of entries) {
    const name = Buffer.from(entryName, "utf8");
    const content = Buffer.from(value, "utf8");
    const crc = crc32(content);
    const header = Buffer.alloc(30);
    header.writeUInt32LE(0x04034b50, 0);
    header.writeUInt16LE(20, 4);
    header.writeUInt16LE(0x0800, 6); // UTF-8 names; compression method is stored.
    header.writeUInt32LE(crc, 14);
    header.writeUInt32LE(content.length, 18);
    header.writeUInt32LE(content.length, 22);
    header.writeUInt16LE(name.length, 26);
    local.push(header, name, content);
    const directory = Buffer.alloc(46);
    directory.writeUInt32LE(0x02014b50, 0);
    directory.writeUInt16LE(20, 4);
    directory.writeUInt16LE(20, 6);
    directory.writeUInt16LE(0x0800, 8);
    directory.writeUInt32LE(crc, 16);
    directory.writeUInt32LE(content.length, 20);
    directory.writeUInt32LE(content.length, 24);
    directory.writeUInt16LE(name.length, 28);
    directory.writeUInt32LE(offset, 42);
    central.push(directory, name);
    offset += header.length + name.length + content.length;
  }
  const centralBytes = Buffer.concat(central);
  const end = Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50, 0);
  end.writeUInt16LE(entries.length, 8);
  end.writeUInt16LE(entries.length, 10);
  end.writeUInt32LE(centralBytes.length, 12);
  end.writeUInt32LE(offset, 16);
  return Buffer.concat([...local, centralBytes, end]);
}

const wordNamespace = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const wordXml = `<w:document xmlns:w="${wordNamespace}"><w:body><w:p><w:r><w:t>Documento de teste com ação.</w:t></w:r></w:p><w:p><w:r><w:t>Parágrafo dois &amp; referência.</w:t></w:r></w:p></w:body></w:document>`;
function docx(documentXml) {
  return zip([
    ["[Content_Types].xml", '<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>'],
    ["word/document.xml", documentXml],
  ]);
}

function pdf() {
  const streams = ["BT /F1 12 Tf 30 100 Td (PDF page one fixture) Tj ET", "BT /F1 12 Tf 30 100 Td (PDF page two fixture) Tj ET"];
  const objects = [
    "<< /Type /Catalog /Pages 2 0 R >>",
    "<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 200] /Resources << /Font << /F1 5 0 R >> >> /Contents 6 0 R >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 200] /Resources << /Font << /F1 5 0 R >> >> /Contents 7 0 R >>",
    "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
    ...streams.map((content) => `<< /Length ${Buffer.byteLength(content, "ascii")} >>\nstream\n${content}\nendstream`),
  ];
  const parts = [Buffer.from("%PDF-1.4\n", "ascii")];
  const offsets = [0];
  let offset = parts[0].length;
  objects.forEach((object, index) => {
    offsets.push(offset);
    const part = Buffer.from(`${index + 1} 0 obj\n${object}\nendobj\n`, "ascii");
    offset += part.length;
    parts.push(part);
  });
  const table = offsets.slice(1).map((position) => `${String(position).padStart(10, "0")} 00000 n \n`).join("");
  parts.push(Buffer.from(`xref\n0 ${objects.length + 1}\n0000000000 65535 f \n${table}trailer\n<< /Size ${objects.length + 1} /Root 1 0 R >>\nstartxref\n${offset}\n%%EOF\n`, "ascii"));
  return Buffer.concat(parts);
}

function launch(executable, cwd, input, { deadlineMs = TIMEOUT_MS, keepInputOpen = false } = {}) {
  return new Promise((resolvePromise, reject) => {
    const child = spawn(executable, ["--coucou-document-worker"], {
      cwd, windowsHide: true, stdio: ["pipe", "pipe", "pipe"],
      env: process.env.SystemRoot ? { SystemRoot: process.env.SystemRoot } : {},
    });
    const chunks = [];
    let bytes = 0;
    let deadline = false;
    let oversized = false;
    let spawnFailed = false;
    let closeDeadline;
    const timer = setTimeout(() => {
      deadline = true;
      child.kill();
      closeDeadline = setTimeout(() => reject(new Error("Worker cleanup did not finish after the harness deadline.")), 3_000);
    }, deadlineMs);
    child.stderr.resume();
    child.stdin.on("error", () => {});
    child.on("error", () => { spawnFailed = true; });
    child.stdout.on("data", (chunk) => {
      bytes += chunk.length;
      if (bytes > RESPONSE_LIMIT) { oversized = true; child.kill(); }
      else chunks.push(chunk);
    });
    child.on("close", (exitCode, signal) => {
      clearTimeout(timer);
      clearTimeout(closeDeadline);
      if (spawnFailed) return reject(new Error("The selected debug worker could not be launched."));
      if (oversized) return reject(new Error("The worker exceeded the bounded response size."));
      if (deadline) return resolvePromise({ deadline: true, closed: true, response: null });
      if (exitCode !== 0 || signal) return reject(new Error("The worker exited without a valid protocol result."));
      try {
        const response = JSON.parse(Buffer.concat(chunks).toString("utf8"));
        resolvePromise({ deadline: false, closed: true, response });
      } catch { reject(new Error("The worker response was not valid JSON.")); }
    });
    if (!keepInputOpen) child.stdin.end(JSON.stringify(input));
  });
}

async function fixture(root, id, name, kind, bytes) {
  const directory = join(root, id);
  await mkdir(directory);
  await writeFile(join(directory, "original.bin"), bytes);
  const attachment = {
    id, conversationId: "document-probe", name, kind,
    size: bytes.length, sha256: createHash("sha256").update(bytes).digest("hex"),
    createdAt: Math.floor(Date.now() / 1000), status: "queued", message: null, coverage: null,
  };
  return { directory, input: { attachment } };
}

function checkExtraction(response, id, marker) {
  assert.equal(response.error, null, "Valid fixture must not produce a parser error.");
  const value = response.extraction;
  assert.ok(value && typeof value.text === "string" && value.text.includes(marker), "Fixture text was not extracted.");
  const chars = [...value.text].length;
  assert.equal(value.coverage.extractedChars, chars, "Coverage must use character offsets.");
  assert.ok(Array.isArray(value.references) && value.references.length > 0, "Extraction needs inspectable references.");
  for (const reference of value.references) {
    assert.equal(reference.attachmentId, id, "Reference belongs to another fixture.");
    assert.ok(reference.id.startsWith(`${id}:r`), "Reference id is not scoped to its attachment.");
    assert.ok(Number.isInteger(reference.offsetChars) && Number.isInteger(reference.endChars), "Reference offsets must be integers.");
    assert.ok(reference.offsetChars >= 0 && reference.endChars > reference.offsetChars && reference.endChars <= chars, "Reference offsets exceed the extraction.");
    assert.ok(typeof reference.label === "string" && reference.label.length > 0, "Reference label is missing.");
  }
  return value;
}

function checkDenied(response) {
  assert.equal(response.extraction, null, "Rejected fixtures cannot expose extracted content.");
  assert.ok(typeof response.error === "string" && response.error.length > 0 && response.error.length <= 1_000, "Rejected fixture needs a bounded error.");
  assert.ok(!response.error.includes("DO_NOT_SHARE"), "Diagnostic reflected the fixture secret.");
}

let ownedDirectory;
let completed = 0;
let activeCase = "setup";
try {
  assert.equal(process.platform, "win32", "This probe qualifies a freshly built Windows executable.");
  assert.equal(process.argv.length, 3, "Pass exactly --executable=<windows/target/{debug,release}/coucou.exe>.");
  assert.ok(process.argv[2].startsWith("--executable="), "An explicit build executable is required.");
  const requested = process.argv[2].slice("--executable=".length);
  assert.ok(requested.length > 0, "The build executable path is empty.");
  const resolved = resolve(requested);
  const permitted = expectedExecutables.find(candidate => normalize(candidate) === normalize(resolved));
  assert.ok(permitted, "Only target/debug or target/release coucou.exe is accepted; root and installed executables are preserved.");
  const executable = await realpath(resolve(requested));
  const expected = await realpath(permitted);
  assert.equal(normalize(executable), normalize(expected), "The build executable does not match its canonical path.");
  const tempRoot = await realpath(tmpdir());
  ownedDirectory = await mkdtemp(join(tempRoot, "coucou-document-probe-"));

  const valid = [
    ["utf8", "ação.txt", "text", Buffer.from("Primeira linha com ação e informação.\nSegunda linha de referência.\n", "utf8"), "ação", null],
    ["json", "fixture.json", "json", Buffer.from(JSON.stringify({ message: "JSON fixture", quantity: 3 }), "utf8"), "JSON fixture", null],
    ["docx", "fixture.docx", "docx", docx(wordXml), "Parágrafo dois & referência.", "paragraph"],
    ["pdf", "fixture.pdf", "pdf", pdf(), "PDF page one fixture", "page"],
  ];
  for (const [id, name, kind, bytes, marker, referenceKind] of valid) {
    activeCase = id;
    const source = await fixture(ownedDirectory, id, name, kind, bytes);
    const { response } = await launch(executable, source.directory, source.input);
    const extraction = checkExtraction(response, id, marker);
    if (referenceKind === "paragraph") assert.ok(extraction.references.some((reference) => reference.paragraph === 2), "DOCX second paragraph is missing.");
    if (referenceKind === "page") {
      assert.equal(extraction.coverage.totalPages, 2);
      assert.equal(extraction.coverage.readPages, 2);
      assert.ok(extraction.text.includes("PDF page two fixture"));
      assert.ok(extraction.references.some((reference) => reference.page === 2), "PDF second page reference is missing.");
    }
    completed++;
    summary({ case: id, status: "pass", extractedChars: extraction.coverage.extractedChars, references: extraction.references.length, totalPages: extraction.coverage.totalPages });
  }

  const denied = [
    ["invalid-xml", "fixture.docx", "docx", docx(`<w:document xmlns:w="${wordNamespace}"><w:body malformed><w:p/></w:body></w:document>`)],
    ["unsupported-image", "fixture.png", "image", Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aRFcAAAAASUVORK5CYII=", "base64")],
    ["secret-after-budget", "fixture.txt", "text", Buffer.from(`${"safe ".repeat(2_500)}\npassword=DO_NOT_SHARE\nsuffix`, "utf8")],
    ["invalid-json", "fixture.json", "json", Buffer.from('{"unterminated":', "utf8")],
  ];
  for (const [id, name, kind, bytes] of denied) {
    activeCase = id;
    const source = await fixture(ownedDirectory, id, name, kind, bytes);
    const { response } = await launch(executable, source.directory, source.input);
    checkDenied(response);
    completed++;
    summary({ case: id, status: "pass", extractionReturned: false });
  }

  activeCase = "integrity";
  const altered = await fixture(ownedDirectory, "integrity", "fixture.txt", "text", Buffer.from("original content", "utf8"));
  await writeFile(join(altered.directory, "original.bin"), "modified content");
  checkDenied((await launch(executable, altered.directory, altered.input)).response);
  completed++;
  summary({ case: "integrity", status: "pass", extractionReturned: false });

  activeCase = "deadline-harness-cleanup";
  const deadlineSource = await fixture(ownedDirectory, "deadline", "fixture.txt", "text", Buffer.from("pending stdin", "utf8"));
  const deadline = await launch(executable, deadlineSource.directory, null, { deadlineMs: 250, keepInputOpen: true });
  assert.ok(deadline.deadline && deadline.closed && deadline.response === null, "Stalled worker must close after the harness deadline.");
  completed++;
  summary({ case: "deadline-harness-cleanup", status: "pass", productionJobQualified: false });
  summary({ status: "pass", passed: completed, modelCalls: 0, guiStarted: false, productionJobQualified: false });
} catch {
  // Protocol errors and source content are deliberately absent from logs.
  summary({ case: activeCase, status: "fail", passed: completed, reason: "A fixture, protocol result or bounded worker lifecycle check failed." });
  process.exitCode = 1;
} finally {
  if (ownedDirectory) {
    const canonical = await realpath(ownedDirectory);
    const tempRoot = await realpath(tmpdir());
    assert.equal(normalize(dirname(canonical)), normalize(tempRoot), "Cleanup must stay inside the temporary directory.");
    assert.ok(canonical.slice(dirname(canonical).length + 1).startsWith("coucou-document-probe-"), "Cleanup requires the owned fixture prefix.");
    await rm(canonical, { recursive: true, force: false });
  }
}
