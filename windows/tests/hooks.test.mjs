import test from "node:test";
import assert from "node:assert/strict";
import { Bridge } from "../src/core/bridge.ts";
import { State } from "../src/core/state.ts";
import { handleAgentEvent, cancelPendingApproval, disposeHookHandlers } from "../src/island/hooks.ts";
import { Island } from "../src/island/island.ts";
import { IslandStateMachine } from "../src/island/fsm.ts";

const event = (changes = {}) => ({
  protocolVersion: 1, agent: "claude", sessionId: "session-a", cwd: "D:\\project",
  eventType: "approvalRequested", toolName: "Bash", approvalTarget: "Bash · echo test",
  requestId: "request-a", requiresApproval: true, ...changes,
});

function fixture(t, realTransitions = false) {
  const originalWindow = globalThis.window;
  const originalRaf = globalThis.requestAnimationFrame;
  const originalAck = Bridge.approvalAck;
  const originalDecline = Bridge.approvalDecline;
  const timers = new Map();
  const frames = [];
  const acknowledgements = [];
  const declines = [];
  let timerId = 0;
  globalThis.window = {
    setTimeout(callback) { const id = ++timerId; timers.set(id, callback); return id; },
    clearTimeout(id) { timers.delete(id); },
    clearInterval() {},
  };
  globalThis.requestAnimationFrame = (callback) => frames.push(callback);
  Bridge.approvalAck = async (id) => { acknowledgements.push(id); };
  Bridge.approvalDecline = async (id) => { declines.push(id); };
  State.tasks = [];
  State.sessions.prune(Number.MAX_SAFE_INTEGER);
  State.focusId = null;
  State.pendingApproval = null;
  State.paused = false;
  State.mode = "compact";
  State.view = "overview";
  State.isPinned = false;
  State.loadIntegrationTasks();
  let island = {
    alert(view) { State.view = view; State.mode = "expanded"; },
    setView(view) { State.view = view; },
    reveal() { if (State.mode === "hidden") State.mode = "compact"; },
    dropPin() { State.isPinned = false; },
  };
  if (realTransitions) {
    // Use the real navigation/FSM methods without creating DOM/canvas nodes.
    island = Object.create(Island.prototype);
    island.fsm = new IslandStateMachine({
      now: () => performance.now(),
      setTimeout: (callback, milliseconds) => globalThis.window.setTimeout(callback, milliseconds),
      clearTimeout: (id) => globalThis.window.clearTimeout(id),
    });
    island.fsm.state = "petit";
    island.fsm.mouseEntered();
    island.wasInIsland = true;
    island.lastPanel = "overview";
    // Retention from pointer/focus/drafts is tested in fsm.test.mjs. This fixture
    // exercises actual navigation and hook presentation without a browser DOM.
    island.syncInteractionRetention = () => {
      island.fsm.userPinned = State.settings.userPinned;
      island.fsm.pinned = State.isPinned || State.pendingApproval != null;
    };
    island.animateGeometry = () => {};
    island.updateWindowCollapsed = () => {};
    island.engine = { resetMorph() {}, setState() {}, triggerEmote() {} };
    island.wireFsm();
  }
  t.after(() => {
    disposeHookHandlers(island);
    Bridge.approvalAck = originalAck;
    Bridge.approvalDecline = originalDecline;
    globalThis.window = originalWindow;
    globalThis.requestAnimationFrame = originalRaf;
  });
  return { island, acknowledgements, declines, timers, paint: () => frames.splice(0).forEach((frame) => frame()) };
}

test("focused Claude approval is acknowledged only after the presentation frame", (t) => {
  const f = fixture(t);
  handleAgentEvent(f.island, event());
  assert.equal(State.view, "approval");
  assert.equal(State.pendingApproval.taskId, "integration_claude");
  assert.equal(State.pendingApproval.sessionId, "session-a");
  assert.deepEqual(f.acknowledgements, []);
  f.paint();
  assert.deepEqual(f.acknowledgements, ["request-a"]);
});

test("compact-to-expanded FSM transition preserves the new approval until presentation", (t) => {
  const f = fixture(t, true);
  handleAgentEvent(f.island, event());
  assert.equal(f.island.fsm.state, "home");
  assert.equal(State.view, "approval");
  assert.equal(State.pendingApproval.requestId, "request-a");
  assert.deepEqual(f.declines, []);
  f.paint();
  assert.deepEqual(f.acknowledgements, ["request-a"]);
});

for (const view of ["settings", "upload", "confused"]) {
  test(`alert(${view}) releases an already acknowledged approval immediately`, (t) => {
    const f = fixture(t, true);
    handleAgentEvent(f.island, event());
    f.paint();
    assert.deepEqual(f.acknowledgements, ["request-a"]);
    f.island.alert(view);
    assert.equal(State.view, view);
    assert.equal(State.pendingApproval, null);
    assert.equal(State.isPinned, false);
    assert.equal(f.island.fsm.pinned, false);
    assert.equal(f.timers.size, 0);
    assert.deepEqual(f.declines, ["request-a"]);
  });
}

test("direct expand also releases a card and confused recovery cannot restore it", (t) => {
  const f = fixture(t, true);
  handleAgentEvent(f.island, event());
  f.paint();
  f.island.expand("settings");
  assert.equal(State.pendingApproval, null);
  assert.deepEqual(f.declines, ["request-a"]);
  handleAgentEvent(f.island, event({ requestId: "request-b" }));
  f.paint();
  f.island.handleDizzy();
  assert.equal(State.view, "confused");
  assert.equal(State.pendingApproval, null);
  const recovery = [...f.timers.values()][0];
  assert.ok(recovery);
  recovery();
  assert.equal(State.view, "overview");
  assert.deepEqual(f.declines, ["request-a", "request-b"]);
});

test("an unfocused session badge never acknowledges or holds its native prompt", (t) => {
  const f = fixture(t);
  State.setFocus("integration_codex");
  handleAgentEvent(f.island, event());
  f.paint();
  assert.equal(State.pendingApproval, null);
  assert.deepEqual(f.acknowledgements, []);
  assert.deepEqual(f.declines, ["request-a"]);
  assert.equal(State.tasks.find((task) => task.id === "integration_claude").pillBadge, "approval");
});

test("a second approval falls back without replacing the displayed card", (t) => {
  const f = fixture(t);
  handleAgentEvent(f.island, event());
  handleAgentEvent(f.island, event({ sessionId: "session-b", requestId: "request-b" }));
  f.paint();
  assert.equal(State.pendingApproval.requestId, "request-a");
  assert.deepEqual(f.acknowledgements, ["request-a"]);
  assert.deepEqual(f.declines, ["request-b"]);
});

test("a competing request in the same session cannot mutate the first card", (t) => {
  const f = fixture(t);
  handleAgentEvent(f.island, event());
  const generation = State.pendingApproval.generation;
  handleAgentEvent(f.island, event({ requestId: "request-b", approvalTarget: "Bash · other command" }));
  assert.equal(State.pendingApproval.requestId, "request-a");
  assert.equal(State.pendingApproval.command, "Bash · echo test");
  assert.equal(State.pendingApproval.generation, generation);
  assert.equal(State.focusTask.state, "approval");
  assert.deepEqual(f.declines, ["request-b"]);
});

test("notification updates during an approval do not leave a ghost card on expiry", (t) => {
  const f = fixture(t);
  handleAgentEvent(f.island, event());
  handleAgentEvent(f.island, event({ eventType: "notification", summary: "Agent notification", requestId: undefined, requiresApproval: false }));
  cancelPendingApproval(f.island);
  assert.equal(State.pendingApproval, null);
  assert.equal(State.focusTask.state, "question");
  assert.equal(State.isPinned, false);
});

test("leaving the card before paint declines rather than acknowledging a hidden request", (t) => {
  const f = fixture(t);
  handleAgentEvent(f.island, event());
  State.view = "prompt";
  f.paint();
  assert.equal(State.pendingApproval, null);
  assert.deepEqual(f.acknowledgements, []);
  assert.deepEqual(f.declines, ["request-a"]);
});

test("pause cleanup and expiry return to the native prompt without a decision", (t) => {
  const f = fixture(t);
  handleAgentEvent(f.island, event());
  f.paint();
  cancelPendingApproval(f.island);
  assert.equal(State.pendingApproval, null);
  assert.equal(State.isPinned, false);
  assert.equal(f.timers.size, 0);
  assert.deepEqual(f.declines, ["request-a"]);
  assert.equal(State.focusTask.state, "question");
  State.paused = true;
  handleAgentEvent(f.island, event({ requestId: "request-paused" }));
  assert.equal(State.pendingApproval, null);
  assert.deepEqual(f.declines, ["request-a", "request-paused"]);
});

test("a stale previous-turn finish cannot cancel the current approval", (t) => {
  const f = fixture(t);
  handleAgentEvent(f.island, event({ eventType: "turnStarted", turnId: "old", requestId: undefined, requiresApproval: false }));
  handleAgentEvent(f.island, event({ eventType: "turnStarted", turnId: "new", requestId: undefined, requiresApproval: false }));
  handleAgentEvent(f.island, event({ turnId: "new" }));
  handleAgentEvent(f.island, event({ eventType: "turnFinished", turnId: "old", requestId: undefined, requiresApproval: false }));
  assert.equal(State.pendingApproval.requestId, "request-a");
  assert.equal(State.focusTask.state, "approval");
  assert.deepEqual(f.declines, []);
});

test("other agents always show a native action and cannot receive Claude approval", (t) => {
  const f = fixture(t);
  State.setFocus("integration_gemini");
  handleAgentEvent(f.island, event({ agent: "gemini", requiresApproval: false, requestId: undefined }));
  f.paint();
  assert.equal(State.pendingApproval, null);
  assert.equal(State.view, "question");
  assert.deepEqual(f.acknowledgements, []);
});
