import test from "node:test";
import assert from "node:assert/strict";
import { buildPrompt } from "../src/views/chat.ts";
import { AgentChat } from "../src/core/agent-chat.ts";
import { Attachments, DOCUMENT_SUMMARY_QUERY } from "../src/core/attachments.ts";
import { Bridge } from "../src/core/bridge.ts";
import { State } from "../src/core/state.ts";

// Minimal DOM fixture for view interactions; no browser, files, CLI or model.
class Element {
  constructor(tag) { this.tagName = tag.toUpperCase(); this.children = []; this.attributes = new Map(); this.handlers = new Map(); this.className = ""; this.value = ""; this.disabled = false; this.hidden = false; this.checked = false; this.dataset = {}; this._text = ""; this.scrollHeight = 0; this.scrollTop = 0; this.clientHeight = 300; this.style = { setProperty() {} }; this.classList = { toggle: (name, enabled) => { const names = new Set(this.className.split(" ").filter(Boolean)); if (enabled) names.add(name); else names.delete(name); this.className = [...names].join(" "); } }; }
  set textContent(value) { this._text = String(value); this.children = []; }
  get textContent() { return this._text + this.children.map((child) => child.textContent).join(""); }
  get firstChild() { return this.children[0]; }
  setAttribute(key, value) { this.attributes.set(key, String(value)); if (key === "value") this.value = String(value); }
  getAttribute(key) { return this.attributes.get(key) ?? null; }
  append(...nodes) { this.children.push(...nodes); }
  removeChild(node) { this.children.splice(this.children.indexOf(node), 1); }
  addEventListener(name, listener) { this.handlers.set(name, [...(this.handlers.get(name) ?? []), listener]); }
  querySelector(selector) { return all(this).find((node) => selector[0] === "." ? node.className.split(" ").includes(selector.slice(1)) : node.tagName === selector.toUpperCase()) ?? null; }
  focus() {} select() {}
  async click() { assert.equal(this.disabled, false, `disabled button: ${this.textContent}`); for (const listener of this.handlers.get("click") ?? []) await listener({ preventDefault() {}, stopPropagation() {} }); await flush(); }
}
function all(element) { return element.children.flatMap((child) => [child, ...all(child)]); }
const flush = () => new Promise((resolve) => setImmediate(resolve));
function fixture(t, agent = "codex") {
  const previousWindow = globalThis.window, previousDocument = globalThis.document;
  globalThis.window = { devicePixelRatio: 1, confirm: () => false };
  globalThis.document = { createElement: (tag) => new Element(tag), createElementNS: (_ns, tag) => new Element(tag), createTextNode: (text) => { const node = new Element("#text"); node.textContent = text; return node; }, activeElement: null };
  const start = Bridge.agentChatStart;
  AgentChat.conversations.splice(0); AgentChat.selectedId = null; AgentChat.setPersonalDraft("");
  AgentChat.statuses = [{ agent, available: true, path: null, detail: "Fixture sem modelo", personalSupported: true }];
  AgentChat.provider = agent; State.mode = "expanded"; State.view = "prompt"; State.paused = false;
  State.loadIntegrationTasks();
  const conversation = AgentChat.selectPersonal(agent);
  t.after(() => { Bridge.agentChatStart = start; Attachments.forget(conversation.contextId); AgentChat.conversations.splice(0); AgentChat.selectedId = null; AgentChat.provider = "codex"; AgentChat.setPersonalDraft(""); globalThis.window = previousWindow; globalThis.document = previousDocument; });
  const view = buildPrompt(() => {}); view.sync();
  const input = view.el.querySelector(".chat-input");
  const button = (text) => all(view.el).find((node) => node.tagName === "BUTTON" && node.textContent === text);
  const files = Attachments.get(conversation.id);
  const attach = (kind = "text", status = "queued") => { files.loaded = true; files.items = [{ id: "document", conversationId: conversation.contextId, name: "nota.txt", kind, size: 10, sha256: "fixture", createdAt: 1, status, message: null, coverage: null }]; files.selected = new Set(status === "unsupported" ? [] : ["document"]); view.sync(); };
  return { view, input, button, conversation, attach };
}

test("chat uses the active main-tab character and contains no provider or conversation selector", (t) => {
  const { view } = fixture(t, "hermes");
  assert.match(view.el.querySelector(".chat-identity").textContent, /Hermes/);
  assert.equal(all(view.el).filter((node) => node.tagName === "SELECT").length, 0);
  assert.equal(view.el.querySelector(".chat-character"), null);
  assert.equal(view.el.querySelector(".chat-advanced").open, undefined);
});

test("sending an empty message with attachments gives a concrete reading action without a remote call", async (t) => {
  const { view, attach, conversation } = fixture(t); attach(); let calls = 0;
  Bridge.agentChatStart = async () => { calls++; };
  await view.el.querySelector(".send-btn").click(); view.sync();
  assert.match(view.el.querySelector(".chat-notice").textContent, /Ler e resumir/);
  assert.equal(calls, 0); assert.equal(conversation.runId, null);
});

test("explicit summarizing sends selected documents once and preserves the unsent draft", async (t) => {
  const { view, attach, input, button } = fixture(t); attach(); const sent = [];
  input.value = "Meu rascunho ainda não está pronto";
  for (const listener of input.handlers.get("input") ?? []) listener({});
  Bridge.agentChatStart = async (request) => { sent.push(request); };
  await button("Ler e resumir").click(); view.sync();
  assert.equal(sent.length, 1); assert.equal(sent[0].query, DOCUMENT_SUMMARY_QUERY);
  assert.deepEqual(sent[0].attachmentIds, ["document"]);
  assert.equal(input.value, "Meu rascunho ainda não está pronto"); assert.equal(AgentChat.personalDraft, input.value);
  assert.equal(button("Ler e resumir").disabled, true);
});

test("summary navigation never replaces an existing draft or sends it automatically", (t) => {
  const { view, input, conversation } = fixture(t);
  input.value = "Texto pessoal preservado";
  for (const listener of input.handlers.get("input") ?? []) listener({});
  Attachments.requestSummary(conversation.id); view.sync();
  assert.equal(input.value, "Texto pessoal preservado"); assert.match(view.el.querySelector(".chat-notice").textContent, /rascunho foi preservado/);
  assert.equal(conversation.runId, null);
});

test("an unsupported image remains visible and cannot be sent as readable text", (t) => {
  const { view, attach, button } = fixture(t); attach("image", "unsupported");
  assert.match(view.el.querySelector(".attachment-chip").textContent, /visão\/OCR indisponível/);
  assert.equal(button("Ler e resumir").disabled, true);
});
