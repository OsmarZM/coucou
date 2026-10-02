import test from "node:test";
import assert from "node:assert/strict";
import { AgentChatStore, AgentChat, PERSONAL_CONTEXT_ID, chatDiagnostic, decideAgentChat, cancelAgentChat, cancelAgentChatApprovals, setApprovalInteractionGuard, sendAgentChat } from "../src/core/agent-chat.ts";
import { Bridge } from "../src/core/bridge.ts";

const make = (store, config = {}) => store.create({ agent: "codex", cwd: "D:\\project", writable: false, ...config });
const event = (request, kind, data) => ({ conversationId: request.conversationId, runId: request.runId, kind, data });

test("diagnostics preserve benign URLs, assignments and paths while refusing credential patterns", () => {
  const benign = "Erro code=403 em https://example.test/help; pasta D:\\coucou\\arquivo.txt não encontrada.";
  assert.equal(chatDiagnostic(benign), benign);
  for (const secret of ["Authorization: Bearer private-token", "API_KEY=private-key", '"clientSecret":"private-value"', "--password private-value", "https://user:private-password@example.test", "ghp_private-token", "database_url=postgres://private"]) {
    assert.equal(chatDiagnostic(secret).includes("private"), false);
    assert.match(chatDiagnostic(secret), /Diagnóstico omitido/);
  }
  assert.equal(chatDiagnostic("Error: token não informado; consulte https://example.test/login"), "token não informado; consulte https://example.test/login");
});

test("character selection reuses isolated personal channels and preserves the global draft without a send", () => {
  const store = new AgentChatStore();
  const codex = store.selectPersonal("codex"); store.setPersonalDraft("texto ainda não enviado");
  const claude = store.selectPersonal("claude");
  assert.notEqual(codex.id, claude.id); assert.equal(codex.contextId, PERSONAL_CONTEXT_ID); assert.equal(claude.contextId, PERSONAL_CONTEXT_ID);
  assert.equal(store.selectPersonal("codex"), codex); assert.equal(store.personalDraft, "texto ainda não enviado");
  assert.deepEqual(codex.messages, []); assert.deepEqual(claude.messages, []);
  assert.equal(codex.runId, null); assert.equal(claude.runId, null); assert.equal(codex.sessionId, null); assert.equal(claude.sessionId, null);
});

test("personal timeline merges providers by timestamp and replaces persisted snapshots without duplicating local messages", () => {
  const store = new AgentChatStore(); const codex = store.selectPersonal("codex");
  const first = store.begin(codex.id, "pergunta"); store.apply(event(first, "delta", { text: "resposta" })); store.apply(event(first, "completed", { status: "completed" }));
  const persisted = codex.messages.map((message) => ({ ...message, conversationId: codex.id, contentTruncated: false }));
  assert.equal(persisted[0].id, `${first.runId}-user`); assert.equal(persisted[1].id, `${first.runId}-assistant`);
  store.applyContextHistory({ contextId: PERSONAL_CONTEXT_ID, text: "dados", messages: [...persisted, { id: "earlier", conversationId: "older-owned-channel", agent: "claude", role: "assistant", content: "contexto anterior", createdAt: 1, contentTruncated: false }], bytes: 100, budget: 65536, truncated: false });
  const claude = store.selectPersonal("claude"); const second = store.begin(claude.id, "continuar");
  store.apply(event(second, "message", { text: "continuação" }));
  assert.deepEqual(store.timeline().map((message) => message.content), ["contexto anterior", "pergunta", "resposta", "continuar", "continuação"]);
  assert.equal(store.timeline().length, 5);
  assert.equal(store.timeline().filter((message) => message.agent === "codex").length, 2);
  assert.equal(store.current, claude); assert.equal(codex.sessionId, null);
});

test("legacy, project and manually resumed channels never join the personal context implicitly", () => {
  const storage = { getItem() { return JSON.stringify({ selectedId: "legacy", conversations: [{ id: "legacy", agent: "codex", cwd: "", sessionId: "external-native-id", writable: false }] }); }, setItem() {} };
  const store = new AgentChatStore(storage); const legacy = store.current;
  assert.equal(legacy.contextId, null);
  const own = store.selectPersonal("codex"); assert.notEqual(own.id, legacy.id); assert.equal(own.sessionId, null);
  const project = make(store); const resume = make(store, { cwd: "", sessionId: "another-external" });
  store.begin(project.id, "project secret"); store.begin(resume.id, "external secret");
  assert.deepEqual(store.timeline(), []); assert.equal(project.contextId, null); assert.equal(resume.contextId, null);
});

test("late context history cannot change selection, draft, sessions or the live response", async (t) => {
  const original = Bridge.historyContext, originalList = Bridge.historyList; t.after(() => { Bridge.historyContext = original; Bridge.historyList = originalList; });
  Bridge.historyList = async () => [];
  const store = new AgentChatStore(); const channel = store.selectPersonal("codex"); store.setPersonalDraft("draft intacto");
  const request = store.begin(channel.id, "pergunta atual"); store.apply(event(request, "delta", { text: "resposta atual" }));
  const snapshot = (content) => ({ contextId: PERSONAL_CONTEXT_ID, text: "dados", messages: [{ id: `${request.runId}-assistant`, conversationId: channel.id, agent: "codex", role: "assistant", content, createdAt: 1, contentTruncated: false }, { id: "older", conversationId: "history-only", agent: "claude", role: "user", content, createdAt: 2, contentTruncated: true }], bytes: 100, budget: 65536, truncated: true });
  const pending = []; Bridge.historyContext = () => new Promise((resolve) => pending.push(resolve));
  const old = store.refreshContextHistory(); const next = store.refreshContextHistory();
  pending[1](snapshot("nova captura")); await next; pending[0](snapshot("captura atrasada")); await old;
  assert.equal(store.current, channel); assert.equal(store.personalDraft, "draft intacto"); assert.equal(channel.runId, request.runId); assert.equal(channel.sessionId, null);
  assert.equal(store.timeline().find((message) => message.id === `${request.runId}-assistant`).content, "resposta atual");
  assert.equal(store.timeline().find((message) => message.id === "older").content, "nova captura");
  assert.equal(store.timeline().find((message) => message.id === "older").contentTruncated, true); assert.equal(store.conversations.length, 1);
});

test("legacy personal metadata joins only an exact backend binding and never selects or changes a native session", () => {
  const store = new AgentChatStore(); const legacy = store.create({ agent: "codex", cwd: "", writable: false, sessionId: "native-owned", contextId: null });
  const external = store.create({ agent: "claude", cwd: "", writable: false, sessionId: "native-external", contextId: null });
  const selected = store.selectPersonal("codex");
  const binding = { conversationId: legacy.id, agent: legacy.agent, sessionId: legacy.sessionId, cwd: null, contextId: PERSONAL_CONTEXT_ID, title: "guard", messageCount: 0, updatedAt: 1 };
  assert.equal(store.adoptPersonalMetadata([{ ...binding, sessionId: "different" }, { ...binding, agent: "claude" }, { ...binding, cwd: "D:\\project" }, { ...binding, contextId: null }]), false);
  assert.equal(legacy.contextId, null);
  assert.equal(store.adoptPersonalMetadata([binding, { ...binding, conversationId: external.id, agent: external.agent, sessionId: external.sessionId, contextId: null }]), true);
  assert.equal(legacy.contextId, PERSONAL_CONTEXT_ID); assert.equal(legacy.sessionId, "native-owned"); assert.equal(external.contextId, null); assert.equal(store.current, selected);
});

test("switching character invalidates an outstanding allow click and never forwards the draft", async (t) => {
  const { conversation, request } = controllerFixture(t); let focus; const decisions = []; let starts = 0;
  const original = Bridge.agentChatStart; t.after(() => { Bridge.agentChatStart = original; });
  Bridge.agentChatStart = async () => { starts++; };
  Bridge.focusWindow = () => new Promise((resolve) => { focus = resolve; }); Bridge.agentChatDecide = async (...args) => { decisions.push(args); return true; };
  AgentChat.setPersonalDraft("não enviar ao trocar");
  const pending = decideAgentChat(conversation.id, request.runId, "permission", "allow");
  AgentChat.selectPersonal("claude"); focus();
  assert.equal(await pending, false); assert.deepEqual(decisions, []); assert.equal(starts, 0); assert.equal(AgentChat.personalDraft, "não enviar ao trocar");
});

test("CLI conversations retain exact folder/capability and independent native IDs", () => {
  const store = new AgentChatStore();
  const a = make(store, { cwd: "D:\\same project" });
  const b = make(store, { cwd: "D:\\same project", writable: true });
  const ar = store.begin(a.id, "first");
  const br = store.begin(b.id, "second");
  assert.notEqual(ar.runId, br.runId);
  store.apply(event(ar, "session", { sessionId: "native-a" }));
  store.apply(event(br, "session", { sessionId: "native-b" }));
  store.apply(event(ar, "delta", { text: "answer-a" }));
  assert.equal(a.sessionId, "native-a");
  assert.equal(b.sessionId, "native-b");
  assert.equal(a.messages.at(-1).content, "answer-a");
  assert.equal(b.messages.at(-1).content, "");
  assert.equal(ar.cwd, "D:\\same project");
  assert.equal(ar.writable, false);
  assert.equal(br.writable, true);
});

test("unknown conversations and stale turns never update a newer turn", () => {
  const store = new AgentChatStore();
  const conversation = make(store);
  const first = store.begin(conversation.id, "first");
  assert.throws(() => store.begin(conversation.id, "overlap"), /turno ativo/);
  assert.equal(store.apply({ ...event(first, "delta", { text: "foreign" }), conversationId: "unknown" }), false);
  assert.equal(store.apply(event(first, "completed", { status: "completed" })), true);
  const next = store.begin(conversation.id, "next");
  assert.equal(store.apply(event(first, "completed", { status: "failed", error: "late" })), false);
  assert.equal(store.apply(event(first, "delta", { text: "late" })), false);
  assert.equal(conversation.runId, next.runId);
  assert.equal(conversation.error, null);
});

test("streaming deltas merge while final message replaces the streamed turn", () => {
  const store = new AgentChatStore();
  const conversation = make(store);
  const request = store.begin(conversation.id, "hello");
  store.apply(event(request, "delta", { text: "one " }));
  store.apply(event(request, "delta", { text: "two" }));
  assert.equal(conversation.messages.at(-1).content, "one two");
  store.apply(event(request, "message", { text: "final" }));
  assert.equal(conversation.messages.at(-1).content, "final");
  store.apply(event(request, "completed", { status: "completed" }));
  assert.equal(conversation.status, "completed");
  assert.equal(conversation.runId, null);
});

test("status notifications report activity without completing a turn or exposing raw credentials", () => {
  const store = new AgentChatStore();
  const conversation = make(store);
  const request = store.begin(conversation.id, "hello");
  assert.equal(store.apply(event(request, "status", { text: "Retrying the current request." })), true);
  assert.equal(conversation.tools.at(-1), "Retrying the current request.");
  assert.equal(conversation.status, "running");
  store.apply(event(request, "status", { text: "Authorization: Bearer private-token" }));
  store.apply(event(request, "error", { message: "API_KEY=private-key" }));
  assert.equal(conversation.tools.at(-1).includes("private-token"), false);
  assert.equal(conversation.error.includes("private-key"), false);
  assert.equal(conversation.runId, request.runId);
  assert.equal(store.apply({ ...event(request, "status", { text: "late" }), runId: "wrong" }), false);
});

test("stopping invalidates permission cards and ignores subsequent work until termination", () => {
  const store = new AgentChatStore();
  const conversation = make(store);
  const request = store.begin(conversation.id, "change");
  const permission = { requestId: "permission-a", title: "Command", detail: "echo ok", choices: ["allow", "deny"] };
  store.apply(event(request, "approval", permission));
  assert.equal(store.markStopping(conversation.id, "wrong-run"), false);
  assert.ok(conversation.approval);
  assert.equal(store.markStopping(conversation.id, request.runId), true);
  assert.equal(conversation.approval, null);
  assert.equal(store.apply(event(request, "approval", permission)), false);
  assert.equal(store.apply(event(request, "delta", { text: "too late" })), false);
  assert.throws(() => store.begin(conversation.id, "new"), /turno ativo/);
  store.apply(event(request, "completed", { status: "interrupted" }));
  assert.equal(conversation.status, "interrupted");
});

test("a failed stop retains turn ownership and cannot unlock a competing send", () => {
  const store = new AgentChatStore();
  const conversation = make(store);
  const request = store.begin(conversation.id, "work");
  store.markStopping(conversation.id, request.runId);
  store.cancelFailed(conversation.id, request.runId, "IPC unavailable");
  assert.equal(conversation.status, "running");
  assert.equal(conversation.runId, request.runId);
  assert.equal(conversation.error, "IPC unavailable");
  assert.throws(() => store.begin(conversation.id, "another"), /turno ativo/);
});

test("approval resolution is exact and oversized or incomplete cards cannot be actionable", () => {
  const store = new AgentChatStore();
  const conversation = make(store);
  const request = store.begin(conversation.id, "work");
  const permission = { requestId: "permission-a", title: "Command", detail: "echo ok", choices: ["allow", "deny"] };
  assert.equal(store.apply(event(request, "approval", { ...permission, choices: ["allow"] })), false);
  assert.equal(store.apply(event(request, "approval", { ...permission, detail: "x".repeat(16_385) })), false);
  assert.equal(conversation.approval, null);
  store.apply(event(request, "approval", permission));
  assert.equal(store.apply(event(request, "approvalResolved", { requestId: "other" })), false);
  assert.equal(conversation.approval.requestId, "permission-a");
  store.apply(event(request, "approvalResolved", { requestId: "permission-a" }));
  assert.equal(conversation.approval, null);
});

test("metadata persistence omits chat messages, tool outputs and permission details", () => {
  let saved = "";
  const storage = { getItem() { return null; }, setItem(_key, value) { saved = value; } };
  const store = new AgentChatStore(storage);
  const conversation = make(store);
  const request = store.begin(conversation.id, "secret query");
  store.apply(event(request, "delta", { text: "secret answer" }));
  store.apply(event(request, "tool", { text: "secret tool output" }));
  store.apply(event(request, "session", { sessionId: "native-resume" }));
  const metadata = JSON.parse(saved);
  assert.equal(metadata.conversations[0].sessionId, "native-resume");
  assert.equal(saved.includes("secret"), false);
  const resumed = new AgentChatStore({ getItem() { return saved; }, setItem() {} });
  assert.equal(resumed.current.sessionId, "native-resume");
  assert.deepEqual(resumed.current.messages, []);
  assert.equal(resumed.current.runId, null);
  assert.equal(resumed.current.status, "idle");
});

test("corrupt metadata and an unavailable storage never block fresh chat", () => {
  const corrupt = new AgentChatStore({ getItem() { return "{"; }, setItem() { throw new Error("denied"); } });
  assert.equal(corrupt.current, null);
  assert.doesNotThrow(() => make(corrupt));
  const duplicate = new AgentChatStore({ getItem() { return JSON.stringify({ conversations: [
    { id: "same", agent: "codex", cwd: "D:\\first", writable: false },
    { id: "same", agent: "codex", cwd: "D:\\second", writable: false },
    { id: "bad", agent: "invented", cwd: "D:\\bad" },
  ] }); }, setItem() {} });
  assert.equal(duplicate.conversations.length, 1);
  assert.equal(duplicate.current.cwd, "D:\\first");
});

test("bounded UI history and metadata never evict an active conversation", () => {
  const store = new AgentChatStore();
  const active = make(store);
  store.begin(active.id, "keep active");
  for (let index = 0; index < 40; index++) make(store, { cwd: `D:\\project-${index}` });
  assert.equal(store.conversations.length, 16);
  assert.ok(store.conversations.includes(active));
  const conversation = store.current;
  for (let index = 0; index < 65; index++) {
    const request = store.begin(conversation.id, "q".repeat(1000));
    store.apply(event(request, "delta", { text: "a".repeat(5000) }));
    store.apply(event(request, "completed", { status: "completed" }));
  }
  assert.ok(conversation.messages.length <= 100);
  assert.ok(conversation.messages.reduce((length, message) => length + message.content.length, 0) <= 65_536);
  const huge = store.begin(conversation.id, "q");
  store.apply(event(huge, "delta", { text: "x".repeat(100_000) }));
  assert.ok(conversation.messages.reduce((length, message) => length + message.content.length, 0) <= 65_536);
});

test("personal chat starts without a project and invalid messages fail before a turn", () => {
  const store = new AgentChatStore();
  const conversation = make(store, { cwd: "" });
  const personal = store.begin(conversation.id, "hello");
  assert.equal(personal.cwd, "");
  assert.equal(personal.writable, false);
  assert.deepEqual(personal.attachmentIds, []);
  const configured = make(store);
  assert.throws(() => store.begin(configured.id, "x".repeat(16_385)), /16.384/);
  assert.equal(configured.runId, null);
  assert.deepEqual(configured.messages, []);
});

test("attachment selection is bounded and captured before asynchronous start", () => {
  const store = new AgentChatStore(); const conversation = make(store, { cwd: "" });
  assert.throws(() => store.begin(conversation.id, "hello", Array.from({ length: 11 }, (_, i) => String(i))));
  assert.throws(() => store.begin(conversation.id, "hello", ["same", "same"]));
  assert.equal(conversation.runId, null);
  const selection = ["file-a", "file-b"];
  const request = store.begin(conversation.id, "hello", selection);
  selection.pop();
  assert.deepEqual(request.attachmentIds, ["file-a", "file-b"]);
});

const usage = (runId, total, scope = "conversation", windows = []) => ({ provider: "codex", sessionId: "native", runId, sourceVersion: null, capturedAt: 100, tokens: { scope, source: "test", quality: "observed", capturedAt: 100, unit: "tokens", input: 10, output: 5, cachedInput: null, cacheCreation: null, reasoning: null, total }, lastTurnTokens: null, context: null, quota: { availability: windows.length ? "observed" : "unavailable", accountScope: null, source: "test", capturedAt: 100, windows, message: null } });

test("usage snapshots replace cumulative totals and missing quota instead of adding them", () => {
  const store = new AgentChatStore(); const conversation = make(store); const request = store.begin(conversation.id, "hello");
  const window = { id: "5h", source: "test", capturedAt: 100, quality: "observed", unit: "percent", usedPercent: 20, remainingPercent: 80, durationMinutes: 300, resetsAt: 200, state: "available" };
  store.apply(event(request, "usage", usage(request.runId, 15, "conversation", [window])));
  store.apply(event(request, "usage", usage(request.runId, 30)));
  assert.equal(conversation.usage.tokens.total, 30);
  assert.deepEqual(conversation.usage.quota.windows, []);
  store.apply(event(request, "usage", usage(request.runId, null, "model_call")));
  assert.equal(conversation.usage.tokens.total, null);
  assert.equal(conversation.usage.tokens.scope, "model_call");
  assert.equal(store.apply(event(request, "usage", usage("stale", 999))), false);
  assert.equal(store.apply(event(request, "usage", usage(request.runId, -1))), false);
  assert.equal(conversation.usage.tokens.total, null);
});

test("activity IDs distinguish tools and subagents and approvals don't clear their execution", () => {
  const store = new AgentChatStore(); const conversation = make(store); const request = store.begin(conversation.id, "hello");
  const activity = { provider: "codex", sessionId: "native", runId: request.runId, id: "same", status: "running", source: "test", capturedAt: 100, quality: "observed" };
  store.apply(event(request, "activity", { ...activity, kind: "tool" }));
  store.apply(event(request, "activity", { ...activity, kind: "subagent" }));
  store.apply(event(request, "activity", { processCount: 2 }));
  store.apply(event(request, "approval", { requestId: "approval", title: "External chat", detail: "Destination: external-id\nMessage: exact full text", choices: ["allow", "deny"] }));
  store.apply(event(request, "approvalResolved", { requestId: "approval" }));
  assert.equal(Object.keys(conversation.activities).length, 2);
  assert.equal(conversation.processCount, 2);
  store.apply(event(request, "completed", { status: "completed" }));
  assert.deepEqual(conversation.activities, {});
  assert.equal(conversation.processCount, 0);
});

test("restoring saved history filters foreign/tool rows and cannot overwrite an active turn", () => {
  const store = new AgentChatStore();
  const session = { conversationId: "saved-conversation", agent: "codex", sessionId: "native", cwd: null, title: "Saved", messageCount: 2, updatedAt: 100 };
  const restored = store.restoreHistory(session, [{ id: "a", conversationId: session.conversationId, role: "user", content: "hello", createdAt: 1 }, { id: "b", conversationId: "other", role: "assistant", content: "foreign", createdAt: 1 }, { id: "c", conversationId: session.conversationId, role: "tool", content: "tool secret", createdAt: 1 }]);
  assert.equal(restored.cwd, ""); assert.equal(restored.writable, false); assert.equal(restored.sessionId, "native");
  assert.equal(restored.messages.length, 1);
  const request = store.begin(restored.id, "active");
  store.restoreHistory(session, []);
  assert.equal(restored.runId, request.runId); assert.equal(restored.messages.at(-2).content, "active");
  assert.throws(() => store.forgetLocal(restored.id));
});

function controllerFixture(t) {
  const decide = Bridge.agentChatDecide;
  const cancel = Bridge.agentChatCancel;
  const focus = Bridge.focusWindow;
  AgentChat.conversations.splice(0);
  AgentChat.selectedId = null;
  AgentChat.provider = "codex";
  setApprovalInteractionGuard(() => true);
  t.after(() => {
    Bridge.agentChatDecide = decide;
    Bridge.agentChatCancel = cancel;
    Bridge.focusWindow = focus;
    AgentChat.conversations.splice(0);
    AgentChat.selectedId = null;
    setApprovalInteractionGuard(() => false);
  });
  const conversation = make(AgentChat);
  const request = AgentChat.begin(conversation.id, "permission test");
  AgentChat.apply(event(request, "approval", { requestId: "permission", title: "Command", detail: "echo ok", choices: ["allow", "deny"] }));
  return { conversation, request };
}

test("permission clicks are scoped to the selected run and cannot be submitted twice", async (t) => {
  const { conversation, request } = controllerFixture(t);
  const calls = [];
  let resolve;
  Bridge.focusWindow = async () => {};
  Bridge.agentChatDecide = (...args) => { calls.push(args); return new Promise((done) => { resolve = done; }); };
  assert.equal(await decideAgentChat(conversation.id, "stale-run", "permission", "allow"), false);
  const other = make(AgentChat);
  assert.equal(await decideAgentChat(conversation.id, request.runId, "permission", "allow"), false);
  AgentChat.select(conversation.id);
  const pending = decideAgentChat(conversation.id, request.runId, "permission", "allow");
  assert.equal(conversation.approval.submitting, true);
  assert.equal(await decideAgentChat(conversation.id, request.runId, "permission", "allow"), false);
  assert.deepEqual(calls, [[conversation.id, request.runId, "permission", "allow"]]);
  resolve(true);
  assert.equal(await pending, true);
  assert.equal(conversation.approval, null);
  assert.equal(other.messages.length, 0);
});

test("an allow click waits for focus and selection changes in that gap cannot grant permission", async (t) => {
  const { conversation, request } = controllerFixture(t);
  let resolveFocus;
  const calls = [];
  Bridge.focusWindow = () => new Promise((resolve) => { resolveFocus = resolve; });
  Bridge.agentChatDecide = async (...args) => { calls.push(args); return true; };
  const allow = decideAgentChat(conversation.id, request.runId, "permission", "allow");
  assert.deepEqual(calls, []);
  make(AgentChat);
  resolveFocus();
  assert.equal(await allow, false);
  assert.deepEqual(calls, []);
  assert.equal(conversation.approval.submitting, false);
});

test("external message approval exposes exact detail and never grants conversation permission", async (t) => {
  const { conversation, request } = controllerFixture(t); const calls = [];
  const detail = "Destination chat ID: external-123\nMessage: Send exactly this text, including <script>literal</script>.";
  AgentChat.apply(event(request, "approval", { requestId: "external-send", title: "Enviar mensagem externa", detail, choices: ["allow", "deny"] }));
  assert.equal(conversation.approval.detail, detail);
  Bridge.focusWindow = async () => {};
  Bridge.agentChatDecide = async (...args) => { calls.push(args); return true; };
  assert.equal(await decideAgentChat(conversation.id, request.runId, "external-send", "allowConversation"), false);
  assert.deepEqual(calls, []);
  assert.equal(await decideAgentChat(conversation.id, request.runId, "external-send", "allow"), true);
  assert.deepEqual(calls, [[conversation.id, request.runId, "external-send", "allow"]]);
});

test("conversation-scoped allowance exists only when the native request includes that choice", async (t) => {
  const { conversation, request } = controllerFixture(t); const calls = [];
  AgentChat.apply(event(request, "approval", { requestId: "folder", title: "Read folder", detail: "Count files in D:\\authorized", choices: ["allow", "allowConversation", "deny"] }));
  Bridge.focusWindow = async () => {};
  Bridge.agentChatDecide = async (...args) => { calls.push(args); return true; };
  assert.equal(await decideAgentChat(conversation.id, request.runId, "folder", "allowConversation"), true);
  assert.deepEqual(calls, [[conversation.id, request.runId, "folder", "allowConversation"]]);
});

test("hidden or paused chat cannot allow an external message even after focus succeeds", async (t) => {
  const { conversation, request } = controllerFixture(t); const calls = [];
  let visible = false; setApprovalInteractionGuard(() => visible);
  Bridge.agentChatDecide = async (...args) => { calls.push(args); return true; };
  assert.equal(await decideAgentChat(conversation.id, request.runId, "permission", "allow"), false);
  assert.deepEqual(calls, []);
  visible = true; let resolveFocus;
  Bridge.focusWindow = () => new Promise((resolve) => { resolveFocus = resolve; });
  const pending = decideAgentChat(conversation.id, request.runId, "permission", "allow");
  visible = false; resolveFocus();
  assert.equal(await pending, false); assert.deepEqual(calls, []);
  assert.equal(conversation.approval.submitting, false);
});

test("start rejection retains the draft and never retries the message automatically", async (t) => {
  const { conversation } = controllerFixture(t);
  // Complete the fixture's prior permission turn before starting a new request.
  AgentChat.apply({ conversationId: conversation.id, runId: conversation.runId, kind: "completed", data: { status: "interrupted" } });
  conversation.draft = "preserve this exact draft";
  const original = Bridge.agentChatStart; let calls = 0;
  t.after(() => { Bridge.agentChatStart = original; });
  Bridge.agentChatStart = async () => { calls++; throw new Error("Synthetic start rejected"); };
  assert.equal(await sendAgentChat(conversation, conversation.draft), false);
  assert.equal(conversation.draft, "preserve this exact draft");
  assert.equal(conversation.runId, null); assert.equal(conversation.status, "failed"); assert.equal(calls, 1);
});

test("hiding chat cards denies pending requests without cancelling ordinary turns", async (t) => {
  const { conversation, request } = controllerFixture(t);
  const ordinary = make(AgentChat);
  const normal = AgentChat.begin(ordinary.id, "ordinary turn");
  const calls = [];
  Bridge.agentChatDecide = async (...args) => { calls.push(args); return true; };
  const pending = cancelAgentChatApprovals();
  assert.equal(conversation.approval, null);
  await pending;
  assert.deepEqual(calls, [[conversation.id, request.runId, "permission", "deny"]]);
  assert.equal(conversation.runId, request.runId);
  assert.equal(ordinary.runId, normal.runId);
  assert.equal(ordinary.status, "running");
});

test("a late failed cancel response cannot unlock or mark a newer turn as failed", async (t) => {
  const { conversation, request } = controllerFixture(t);
  let resolve;
  Bridge.agentChatCancel = () => new Promise((done) => { resolve = done; });
  const cancel = cancelAgentChat(conversation.id, request.runId);
  assert.equal(conversation.status, "stopping");
  AgentChat.apply(event(request, "completed", { status: "interrupted" }));
  const next = AgentChat.begin(conversation.id, "next");
  resolve(false);
  assert.equal(await cancel, false);
  assert.equal(conversation.runId, next.runId);
  assert.equal(conversation.status, "running");
  assert.equal(conversation.error, null);
});
