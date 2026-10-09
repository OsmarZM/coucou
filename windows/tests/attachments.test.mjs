import test from "node:test";
import assert from "node:assert/strict";
import { AttachmentStore, attachmentStatusLabel } from "../src/core/attachments.ts";
const deferred = () => { let resolve, reject; const promise = new Promise((yes, no) => { resolve = yes; reject = no; }); return { promise, resolve, reject }; };
const item = (conversationId, id, status = "queued") => ({ id, conversationId, name: `${id}.txt`, kind: "text", size: 10, sha256: "hash", createdAt: 1, status, message: null, coverage: null });

test("shared personal context keeps selected attachments across characters but isolates project files", async () => {
  const calls = [];
  const scope = (id) => ["codex-channel", "claude-channel"].includes(id) ? "personal-main" : id;
  const store = new AttachmentStore({ documentsList: async () => [], documentsIngest: async (id, paths) => { calls.push([id, paths]); return [item(id, "shared")]; }, documentsRemove: async (id, file) => { calls.push([id, file]); } }, () => {}, () => {}, scope);
  await store.ingest("codex-channel", ["D:\\file.txt"]);
  assert.deepEqual(calls[0], ["personal-main", ["D:\\file.txt"]]);
  assert.deepEqual(store.selectedIds("claude-channel"), ["shared"]); assert.deepEqual(store.selectedIds("project-channel"), []);
  store.applyPrepared({ conversationId: "claude-channel", runId: "run", kind: "attachments", data: { attachments: [item("personal-main", "shared", "ready"), item("project-channel", "foreign", "ready")] } });
  assert.deepEqual(store.selectedIds("codex-channel"), ["shared"]); assert.equal(store.get("codex-channel").items[0].status, "ready");
  await store.remove("claude-channel", "shared"); assert.deepEqual(calls[1], ["personal-main", "shared"]); assert.deepEqual(store.selectedIds("codex-channel"), []);
});

test("late imports stay attached to the captured conversation", async () => {
  const pending = deferred(); const calls = []; const busy = [];
  const store = new AttachmentStore({ documentsList: async () => [], documentsIngest: (id, paths) => { calls.push([id, paths]); return pending.promise; }, documentsRemove: async () => {} }, () => {}, (value) => busy.push(value));
  const paths = ["D:\\one.txt", "D:\\two.txt"];
  const importing = store.ingest("first", paths); paths.splice(0);
  store.get("second");
  await Promise.resolve();
  pending.resolve([item("first", "a"), item("first", "b")]); await importing;
  assert.deepEqual(calls, [["first", ["D:\\one.txt", "D:\\two.txt"]]]);
  assert.equal(store.get("first").items.length, 2);
  assert.deepEqual(store.selectedIds("first"), ["a", "b"]);
  assert.deepEqual(store.get("second").items, []);
  assert.deepEqual(busy, [true, false]);
});

test("a stale list response cannot erase a completed import", async () => {
  const stale = deferred();
  const store = new AttachmentStore({ documentsList: () => stale.promise, documentsIngest: async (id) => [item(id, "new")], documentsRemove: async () => {} });
  const reading = store.refresh("conversation");
  await store.ingest("conversation", ["D:\\new.txt"]);
  stale.resolve([]); await reading;
  assert.deepEqual(store.get("conversation").items.map((entry) => entry.id), ["new"]);
});

test("an accepted preparation invalidates an older document list and preserves coverage", async () => {
  const stale = deferred();
  const store = new AttachmentStore({ documentsList: () => stale.promise, documentsIngest: async () => [], documentsRemove: async () => {} });
  const reading = store.refresh("conversation");
  const prepared = { ...item("conversation", "file", "ready"), coverage: { extractedChars: 20, totalPages: 3, readPages: 2, emptyPages: [], truncated: true, notes: [] } };
  store.applyPrepared({ conversationId: "conversation", runId: "run", kind: "attachments", data: { attachments: [prepared], usedChars: 20, budgetChars: 100, partial: true } });
  stale.resolve([item("conversation", "file", "queued")]); await reading;
  assert.equal(store.get("conversation").items[0].status, "ready"); assert.equal(store.get("conversation").items[0].coverage.readPages, 2); assert.equal(store.get("conversation").preparation.partial, true);
});

test("imports and removal serialize per conversation while busy remains retained", async () => {
  const first = deferred(); const calls = []; const busy = [];
  const store = new AttachmentStore({ documentsList: async () => [], documentsIngest: (id) => { calls.push("ingest"); return first.promise; }, documentsRemove: async () => { calls.push("remove"); } }, () => {}, (value) => busy.push(value));
  const importing = store.ingest("conversation", ["D:\\file.txt"]);
  const removing = store.remove("conversation", "a");
  await Promise.resolve(); await Promise.resolve();
  assert.deepEqual(calls, ["ingest"]);
  first.resolve([item("conversation", "a")]); await importing; await removing;
  assert.deepEqual(calls, ["ingest", "remove"]);
  assert.deepEqual(store.get("conversation").items, []);
  assert.equal(busy.at(-1), false);
});

test("unsupported documents are never silently selected for text dispatch", async () => {
  const store = new AttachmentStore({ documentsList: async () => [], documentsIngest: async (id) => [item(id, "image", "unsupported"), item(id, "text")], documentsRemove: async () => {} });
  await store.ingest("conversation", ["D:\\image.png", "D:\\text.txt"]);
  store.toggle("conversation", "image", true);
  assert.deepEqual(store.selectedIds("conversation"), ["text"]);
  store.applyPrepared({ conversationId: "conversation", runId: "run", kind: "attachments", data: { attachments: [item("conversation", "text", "ready")], usedChars: 10, budgetChars: 100, partial: true } });
  assert.equal(store.get("conversation").items.find((entry) => entry.id === "text").status, "ready");
  assert.equal(store.get("conversation").preparation.partial, true);
  assert.equal(store.get("conversation").items.length, 2);
});

test("file limit rejection occurs before an oversized import reaches IPC", async () => {
  let calls = 0;
  const store = new AttachmentStore({ documentsList: async () => [], documentsIngest: async () => { calls++; return []; }, documentsRemove: async () => {} });
  await assert.rejects(store.ingest("conversation", Array(11).fill("path")));
  assert.equal(calls, 0);
  assert.equal(store.get("conversation").busy, 0);
});

test("import progress remains pending until IPC confirms local attachment, with an owned operation ID", async () => {
  const pending = deferred(); let operationId;
  const store = new AttachmentStore({ documentsList: async () => [], documentsIngest: (id, paths, operation) => { operationId = operation; return pending.promise; }, documentsRemove: async () => {} });
  const importing = store.ingest("conversation", ["D:\\file.txt"]);
  await Promise.resolve(); await Promise.resolve();
  assert.equal(store.lastImport.status, "importing"); assert.equal(store.get("conversation").busy, 1);
  assert.equal(operationId, store.lastImport.id); assert.match(operationId, /^[a-f0-9-]{36}$/);
  assert.deepEqual(store.selectedIds("conversation"), []);
  pending.resolve([item("conversation", "new")]); await importing;
  assert.equal(store.lastImport.status, "completed"); assert.deepEqual(store.lastImport.attachmentIds, ["new"]);
  assert.equal(store.get("conversation").busy, 0); assert.equal(store.get("conversation").items[0].status, "queued");
});

test("cancelling waits for the ingestion result and is scoped to its exact operation", async () => {
  const pending = deferred(); const cancelled = [];
  const store = new AttachmentStore({ documentsList: async () => [], documentsIngest: () => pending.promise, documentsCancel: async (id) => { cancelled.push(id); return true; }, documentsRemove: async () => {} });
  const importing = store.ingest("first", ["D:\\file.txt"]); const rejection = assert.rejects(importing, /cancelada/);
  await Promise.resolve(); await Promise.resolve();
  store.get("second");
  assert.equal(await store.cancelImport("second"), false);
  assert.equal(await store.cancelImport("first"), true);
  assert.equal(await store.cancelImport("first"), false);
  assert.deepEqual(cancelled, [store.lastImport.id]);
  assert.equal(store.get("first").busy, 1); assert.equal(store.lastImport.status, "cancelling");
  pending.reject(new Error("Importação cancelada pelo usuário.")); await rejection;
  assert.equal(store.get("first").busy, 0); assert.equal(store.lastImport.status, "cancelled");
  assert.deepEqual(store.get("second").items, []); assert.match(store.get("first").error, /cancelada/);
});

test("an unconfirmed cancellation retains busy state and reports its uncertainty", async () => {
  const pending = deferred();
  const store = new AttachmentStore({ documentsList: async () => [], documentsIngest: () => pending.promise, documentsCancel: async () => false, documentsRemove: async () => {} });
  const importing = store.ingest("conversation", ["D:\\file.txt"]);
  await Promise.resolve(); await Promise.resolve();
  assert.equal(await store.cancelImport("conversation"), false); assert.equal(store.get("conversation").busy, 1);
  assert.equal(store.lastImport.status, "importing"); assert.match(store.get("conversation").error, /confirmar o cancelamento/);
  pending.resolve([item("conversation", "done")]); await importing;
  assert.equal(store.lastImport.status, "completed"); assert.equal(store.get("conversation").busy, 0);
});

test("import failures remain visible after refreshing documents and preserve previous attachments", async () => {
  const prior = item("conversation", "previous");
  const store = new AttachmentStore({ documentsList: async () => [prior], documentsIngest: async () => { throw new Error("Arquivo OneDrive indisponível para cópia local."); }, documentsRemove: async () => {} });
  await store.refresh("conversation"); store.toggle("conversation", "previous", true);
  await assert.rejects(store.ingest("conversation", ["D:\\offline.txt"]), /OneDrive/);
  await store.refresh("conversation");
  assert.match(store.get("conversation").error, /OneDrive/); assert.equal(store.lastImport.status, "failed");
  assert.deepEqual(store.selectedIds("conversation"), ["previous"]); assert.equal(store.get("conversation").busy, 0);
});

test("empty or foreign import results never claim success or select a foreign document", async () => {
  for (const response of [[], [item("foreign", "secret")]]) {
    const store = new AttachmentStore({ documentsList: async () => [], documentsIngest: async () => response, documentsRemove: async () => {} });
    await assert.rejects(store.ingest("conversation", ["D:\\file.txt"]), /não confirmou/);
    assert.equal(store.lastImport.status, "failed"); assert.equal(store.get("conversation").busy, 0);
    assert.deepEqual(store.get("conversation").items, []); assert.deepEqual(store.selectedIds("foreign"), []);
  }
});

test("a simultaneous duplicate import is refused before a second IPC call", async () => {
  const pending = deferred(); let calls = 0;
  const store = new AttachmentStore({ documentsList: async () => [], documentsIngest: () => { calls++; return pending.promise; }, documentsRemove: async () => {} });
  const first = store.ingest("conversation", ["D:\\file.txt"]);
  await assert.rejects(store.ingest("conversation", ["D:\\file.txt"]), /Já existe/);
  await Promise.resolve(); await Promise.resolve(); assert.equal(calls, 1);
  pending.resolve([item("conversation", "once")]); await first;
  assert.equal(store.get("conversation").items.length, 1);
});

test("summary intent follows the captured context and is consumed only once without starting a model", () => {
  let calls = 0;
  const store = new AttachmentStore({ documentsList: async () => { calls++; return []; }, documentsIngest: async () => { calls++; return []; }, documentsRemove: async () => { calls++; } }, () => {}, () => {}, (id) => ["codex", "hermes"].includes(id) ? "personal-main" : id);
  store.requestSummary("codex");
  assert.equal(store.takeSummaryRequest("project"), false); assert.equal(store.takeSummaryRequest("hermes"), true);
  assert.equal(store.takeSummaryRequest("codex"), false); assert.equal(calls, 0);
});

test("attachment labels distinguish local copy, partial extraction and unsupported vision", () => {
  assert.match(attachmentStatusLabel(item("c", "file")), /leitura pendente/);
  assert.equal(attachmentStatusLabel({ ...item("c", "file", "ready"), coverage: { truncated: true } }), "Leitura parcial");
  assert.equal(attachmentStatusLabel({ ...item("c", "file", "unsupported"), kind: "image" }), "Imagem anexada · visão/OCR indisponível");
});
