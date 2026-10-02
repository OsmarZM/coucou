import test from "node:test";
import assert from "node:assert/strict";
import {
  SessionStore, sessionKey, readAgentEvent, SESSION_LIMIT, SESSION_TTL_MS, SUMMARY_LIMIT, STEP_LIMIT,
} from "../src/core/sessions.ts";

const event = (changes = {}) => ({
  protocolVersion: 1, agent: "claude", sessionId: "session-a", cwd: "D:\\work\\same-project",
  eventType: "turnStarted", requiresApproval: false, ...changes,
});

test("session identity isolates agents and sessions sharing the same project", () => {
  const store = new SessionStore();
  store.apply(event({ turnId: "turn-a" }), 1);
  store.apply(event({ agent: "codex", turnId: "turn-b" }), 2);
  store.apply(event({ sessionId: "session-b", turnId: "turn-c" }), 3);
  store.apply(event({ eventType: "turnFailed", turnId: "turn-a" }), 4);
  assert.equal(store.size, 3);
  assert.equal(store.get(sessionKey("claude", "session-a")).state, "error");
  assert.equal(store.get(sessionKey("codex", "session-a")).state, "thinking");
  assert.equal(store.get(sessionKey("claude", "session-b")).state, "thinking");
  assert.notEqual(sessionKey("claude", "a:codex:b"), sessionKey("codex", "b"));
});

test("a late previous-turn completion cannot overwrite current activity", () => {
  const store = new SessionStore();
  store.apply(event({ turnId: "old" }), 1);
  const current = store.apply(event({ turnId: "new" }), 2);
  const generation = current.generation;
  assert.equal(store.apply(event({ eventType: "turnFinished", turnId: "old" }), 3), null);
  assert.equal(current.state, "thinking");
  assert.equal(current.turnId, "new");
  assert.equal(current.generation, generation);
});

test("completion timer belongs to its generation and preserves the real outcome", () => {
  const store = new SessionStore();
  const finished = store.apply(event({ eventType: "turnFinished", turnId: "one" }), 1);
  const generation = finished.generation;
  assert.equal(store.clearBadge(finished.key, generation), true);
  assert.equal(finished.state, "finished");
  store.apply(event({ turnId: "two" }), 2);
  store.apply(event({ eventType: "approvalRequested", turnId: "two", requiresApproval: true }), 3);
  assert.equal(store.clearBadge(finished.key, generation), false);
  assert.equal(finished.state, "approval");
  assert.equal(finished.pillBadge, "approval");
});

test("expiry returns an approval to native action without granting permission", () => {
  const store = new SessionStore();
  const approval = store.apply(event({ eventType: "approvalRequested", requiresApproval: true }), 1);
  store.releaseApproval(approval.key, approval.generation);
  assert.equal(approval.state, "question");
  assert.equal(approval.pillBadge, "approval");
  store.apply(event({ eventType: "toolStarted" }), 2);
  store.releaseApproval(approval.key, approval.generation - 1);
  assert.equal(approval.state, "working");
});

test("successful explicit decision cannot be reverted by card cleanup", () => {
  const store = new SessionStore();
  const approval = store.apply(event({ eventType: "approvalRequested", requiresApproval: true }), 1);
  store.decisionSent(approval.key, approval.generation);
  store.releaseApproval(approval.key, approval.generation);
  assert.equal(approval.state, "working");
  assert.equal(approval.pillBadge, null);
});

test("session memory is bounded and a pending card survives pruning", () => {
  const store = new SessionStore();
  const protectedSession = store.apply(event(), 0);
  for (let i = 0; i < SESSION_LIMIT * 2; i++) {
    store.apply(event({ sessionId: `extra-${i}` }), i + 1, protectedSession.key);
  }
  assert.equal(store.size, SESSION_LIMIT);
  assert.equal(store.get(protectedSession.key), protectedSession);
  store.prune(SESSION_TTL_MS + 1000, protectedSession.key);
  assert.equal(store.size, 1);
  store.prune(SESSION_TTL_MS + 1001);
  assert.equal(store.size, 0);
});

test("summaries and tool logs cannot grow without bound", () => {
  const store = new SessionStore();
  for (let i = 0; i < STEP_LIMIT * 4; i++) {
    store.apply(event({ eventType: "toolStarted", summary: `${i} ${"x".repeat(2000)}` }), i);
  }
  const session = store.list()[0];
  assert.equal(session.steps.length, STEP_LIMIT);
  assert.ok(session.steps.every((step) => step.length <= SUMMARY_LIMIT));
  assert.ok(session.steps.at(-1).startsWith("79 "));
});

test("IPC validation requires identity and refuses unsupported schemas", () => {
  assert.equal(readAgentEvent(event({ sessionId: "" })), null);
  assert.equal(readAgentEvent(event({ sessionId: " " })), null);
  assert.equal(readAgentEvent(event({ protocolVersion: 2 })), null);
  assert.equal(readAgentEvent(event({ agent: "unknown" })), null);
  assert.equal(readAgentEvent(event({ eventType: "successProbably" })), null);
  const normalized = readAgentEvent(event({ agent: "copilot", requiresApproval: true, summary: "x".repeat(5000) }));
  assert.equal(normalized.requiresApproval, false);
  assert.equal(normalized.summary.length, SUMMARY_LIMIT);
});

test("IPC validation preserves exact IDs, paths and the approval target", () => {
  const payload = event({
    sessionId: "native  session", turnId: "native  turn", requestId: "native  request",
    cwd: "D:\\My  Project", approvalTarget: "Bash · echo 'two  spaces'",
  });
  const normalized = readAgentEvent(payload);
  assert.equal(normalized.sessionId, payload.sessionId);
  assert.equal(normalized.turnId, payload.turnId);
  assert.equal(normalized.requestId, payload.requestId);
  assert.equal(normalized.cwd, payload.cwd);
  assert.equal(normalized.approvalTarget, payload.approvalTarget);
  assert.equal(readAgentEvent(event({ sessionId: " leading-space" })), null);
  assert.equal(readAgentEvent(event({ turnId: "line\nbreak" })), null);
});

test("tool failure and interruption do not imply a successful completed turn", () => {
  const store = new SessionStore();
  const session = store.apply(event(), 1);
  store.apply(event({ eventType: "toolFailed" }), 2);
  assert.equal(session.state, "working");
  store.apply(event({ eventType: "interrupted" }), 3);
  assert.equal(session.state, "idle");
  assert.equal(session.pillBadge, null);
  assert.equal(session.ended, false);
  store.apply(event({ eventType: "sessionEnded" }), 4);
  assert.equal(session.ended, true);
});
