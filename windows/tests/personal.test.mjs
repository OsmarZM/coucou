import test from "node:test";
import assert from "node:assert/strict";
import { PersonalStore } from "../src/core/personal.ts";
import { Bridge } from "../src/core/bridge.ts";
import { AgentChat } from "../src/core/agent-chat.ts";
import { State } from "../src/core/state.ts";
import { observedNumber } from "../src/views/usage.ts";
import { rateWindowTitle, reportedDuration } from "../src/core/usage-format.ts";
const deferred = () => { let resolve; const promise = new Promise((yes) => { resolve = yes; }); return { promise, resolve }; };
function mock(t, name, value) { const original = Bridge[name]; Bridge[name] = value; t.after(() => { Bridge[name] = original; }); }

test("automatic learning defaults on and a late boot read cannot undo an explicit mutation", async (t) => {
  const read = deferred(); const store = new PersonalStore(() => {});
  assert.deepEqual(store.preferences, { learningEnabled: true, persistHistory: true, autoSaveUserFacts: true });
  mock(t, "memoryPreferences", () => read.promise); mock(t, "focusWindow", async () => {});
  mock(t, "memorySetPreferences", async (preferences) => preferences);
  const pending = store.loadPreferences();
  await store.setPreferences({ learningEnabled: true, persistHistory: true, autoSaveUserFacts: false });
  read.resolve({ learningEnabled: false, persistHistory: false, autoSaveUserFacts: false }); await pending;
  assert.equal(store.preferences.learningEnabled, true); assert.equal(store.preferences.persistHistory, true);
});

test("rapid searches ignore old results and preserve the newer request's busy state", async (t) => {
  const old = deferred(), current = deferred(); const store = new PersonalStore(() => {});
  mock(t, "memoryList", (query) => query.query === "old" ? old.promise : current.promise);
  store.query = "old"; const first = store.refresh();
  store.query = "current"; const second = store.refresh();
  old.resolve([{ id: "stale" }]); await first;
  assert.equal(store.busy, true); assert.deepEqual(store.memories, []);
  current.resolve([{ id: "new" }]); await second;
  assert.equal(store.busy, false); assert.deepEqual(store.memories, [{ id: "new" }]);
});

test("history search deduplicates sessions without conflating hit excerpts", async (t) => {
  const store = new PersonalStore(() => {}); store.tab = "history"; store.query = "needle";
  mock(t, "historySearch", async () => [{ session: { conversationId: "a" }, messageId: "one", excerpt: "first" }, { session: { conversationId: "a" }, messageId: "two", excerpt: "second" }, { session: { conversationId: "b" }, messageId: "three", excerpt: "third" }]);
  await store.refresh();
  assert.deepEqual(store.sessions.map((session) => session.conversationId), ["a", "b"]);
  assert.equal(store.searchHits.length, 3);
});

test("a late history response cannot redirect a newer character choice or navigation", async (t) => {
  const read = deferred(); const store = new PersonalStore(() => {});
  const oldConversations = [...AgentChat.conversations], oldSelected = AgentChat.selectedId, oldProvider = AgentChat.provider, oldView = State.view;
  t.after(() => { AgentChat.conversations.splice(0, AgentChat.conversations.length, ...oldConversations); AgentChat.selectedId = oldSelected; AgentChat.provider = oldProvider; State.view = oldView; });
  AgentChat.conversations.splice(0); AgentChat.selectPersonal("codex"); State.view = "learned";
  mock(t, "historyMessages", () => read.promise);
  const pending = store.resume({ conversationId: "historical", agent: "codex", sessionId: "external", cwd: "D:\\project", contextId: null, title: "old", messageCount: 1, updatedAt: 1 });
  State.view = "prompt"; const chosen = AgentChat.selectPersonal("claude"); State.view = "learned";
  read.resolve([{ id: "old", conversationId: "historical", role: "assistant", content: "stale history", createdAt: 1 }]);
  assert.equal(await pending, null); assert.equal(AgentChat.current, chosen); assert.equal(AgentChat.provider, "claude"); assert.equal(AgentChat.conversations.some((entry) => entry.id === "historical"), false);
});

test("missing usage stays unavailable while observed zero is still zero", () => {
  assert.equal(observedNumber(null), "Indisponível");
  assert.equal(observedNumber(undefined), "Indisponível");
  assert.equal(observedNumber(NaN), "Indisponível");
  assert.equal(observedNumber(0), "0");
  assert.equal(observedNumber(20.5, "%"), "20,5%");
});

test("quota window labels use only the provider's reported duration", () => {
  assert.equal(rateWindowTitle({ id: "primary", durationMinutes: 300 }), "Janela de 5h");
  assert.equal(rateWindowTitle({ id: "secondary", durationMinutes: 10080 }), "Janela semanal (7 dias)");
  assert.equal(rateWindowTitle({ id: "custom", durationMinutes: 90 }), "Janela de 90 min");
  assert.equal(rateWindowTitle({ id: "primary", durationMinutes: null }), "primary");
  assert.equal(reportedDuration(null), "Indisponível");
  assert.equal(reportedDuration(300), "300 min");
});
