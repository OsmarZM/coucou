// Protocol qualification: no model turn, account read or global config write.
import { spawn } from "node:child_process";
import { createInterface } from "node:readline";
import { dirname, join, resolve, sep } from "node:path";
import { existsSync, readFileSync } from "node:fs";
import { mkdir } from "node:fs/promises";
const cwd = resolve("target/personal-protocol-workspace");
await mkdir(cwd, { recursive: true });
// Match the Rust adapter's PATH order; a forced codex.exe may select the Desktop
// bundle instead of the installed npm CLI, which can have a different version.
const launcher = (process.env.PATH ?? "").split(";").flatMap(directory =>
  [".exe", ".cmd", ".ps1"].map(extension => join(directory.replace(/^"|"$/g, ""), "codex" + extension)))
  .find(existsSync);
if (!launcher) throw new Error("Codex CLI is not on PATH.");
let program = launcher, prefix = [];
if (!launcher.toLowerCase().endsWith(".exe")) {
  const directory = resolve(dirname(launcher), "node_modules", "@openai", "codex");
  const metadata = JSON.parse(readFileSync(join(directory, "package.json"), "utf8"));
  const bin = typeof metadata.bin === "string" ? metadata.bin : metadata.bin?.codex;
  if (typeof bin !== "string") throw new Error("Official CLI package has no executable.");
  const script = resolve(directory, bin);
  if (!script.startsWith(directory + sep) || !existsSync(script)) throw new Error("Invalid CLI package executable.");
  program = process.execPath;
  prefix = [script];
}
console.log(JSON.stringify({ launcher }));
const child = spawn(program, [...prefix, "app-server", "--listen", "stdio://"], { cwd, windowsHide: true, stdio: ["pipe", "pipe", "pipe"] });
child.stderr.resume();
const pending = new Map();
let sequence = 0;
createInterface({ input: child.stdout }).on("line", line => {
  try {
    const value = JSON.parse(line);
    if (value.id && pending.has(value.id)) {
      const { resolve, reject, timer } = pending.get(value.id); pending.delete(value.id); clearTimeout(timer);
      if (value.error) reject(new Error("RPC rejected (" + value.error.code + "); no retry."));
      else resolve(value.result);
    }
  } catch {}
});
function request(method, params) {
  const id = "probe-" + ++sequence;
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => { pending.delete(id); reject(new Error("RPC timeout")); }, 20000);
    pending.set(id, { resolve, reject, timer });
    child.stdin.write(JSON.stringify({ id, method, params }) + "\n");
  });
}
try {
  const initialized = await request("initialize", { clientInfo: { name: "coucou_personal_probe", version: "0.1.2" }, capabilities: { experimentalApi: true } });
  console.log(JSON.stringify({ protocolAgent: initialized.userAgent }));
  child.stdin.write(JSON.stringify({ method: "initialized", params: {} }) + "\n");
  const config = await request("config/read", { includeLayers: false });
  const overrides = {
    "features.shell_tool": false, "features.unified_exec": false, "features.apps": false,
    "features.code_mode": false, "features.code_mode_host": false, "features.browser_use": false,
    "features.computer_use": false, "features.view_image": false, "features.multi_agent": false,
    "features.hooks": false, "features.remote_plugin": false, "features.skill_search": false,
    "features.workspace_dependencies": false, "features.shell_snapshot": false,
    "features.shell_snapshot_v2": false, "features.in_app_local_automation": false,
    "features.agent_message_board": false, "features.send_message_to_user_async": false,
    "features.skip_host_skill_discovery": true, "project_doc_max_bytes": 0
  };
  for (const section of ["mcp_servers", "plugins"]) {
    for (const name of Object.keys(config.config?.[section] ?? {})) {
      if (!/^[A-Za-z0-9_@-]{1,200}$/.test(name)) throw new Error("Inherited integration cannot be disabled safely.");
      overrides[`${section}.${name}.enabled`] = false;
    }
  }
  const result = await request("thread/start", {
    cwd, ephemeral: true, environments: [], sandbox: "read-only", approvalPolicy: "on-request", approvalsReviewer: "user",
    config: overrides,
    dynamicTools: [{ type: "function", name: "coucou_count_files", description: "Count files with explicit Coucou approval.", inputSchema: { type: "object", properties: { folder: { type: "string" } }, required: ["folder"], additionalProperties: false } }]
  });
  if (!result.thread?.id || !Array.isArray(result.thread.environments) || result.thread.environments.length !== 0) throw new Error("CLI did not confirm an empty environment.");
  console.log(JSON.stringify({ environments: result.thread.environments, sandbox: result.sandbox?.type, modelTurnStarted: false }));
  await request("thread/unsubscribe", { threadId: result.thread.id });
} catch (error) { console.log(JSON.stringify({ success: false, reason: String(error.message) })); process.exitCode = 1; }
finally { child.stdin.end(); setTimeout(() => child.kill(), 1000).unref(); }
