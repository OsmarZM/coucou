import test from "node:test";
import assert from "node:assert/strict";
import { AppState } from "../src/core/state.ts";
import { AGENT_IDS, AGENT_META, sessionKey } from "../src/core/sessions.ts";

const event = (sessionId, changes = {}) => ({
  protocolVersion: 1, agent: "claude", sessionId, cwd: "D:\\shared",
  eventType: "turnStarted", requiresApproval: false, ...changes,
});

test("agent groups stay visible independently of the four-service limit", () => {
  const state = new AppState();
  state.loadIntegrationTasks();
  assert.equal(state.tasks.length, 8);
  assert.ok(AGENT_IDS.every((agent) => state.tasks.some((task) => task.id === AGENT_META[agent].taskId)));
  state.toggleIntegration("integration_notion");
  assert.equal(state.settings.activeIntegrations.length, 4);
  state.toggleIntegration("integration_codex");
  assert.equal(state.tasks.length, 8);
});

test("new sessions never steal selection and each selection shows its own state", () => {
  const state = new AppState();
  state.loadIntegrationTasks();
  state.sessions.apply(event("first"), 1);
  state.syncAgentTasks();
  state.sessions.apply(event("second", { eventType: "turnFailed" }), 2);
  state.syncAgentTasks();
  assert.equal(state.focusTask.sessionId, "first");
  assert.equal(state.focusTask.state, "thinking");
  assert.equal(state.focusTask.pillBadge, "error");
  state.selectSession("claude", sessionKey("claude", "second"));
  assert.equal(state.focusTask.sessionId, "second");
  assert.equal(state.focusTask.state, "error");
  assert.equal(state.focusTask.name, "Claude Code");
});

test("loading service preferences preserves selected agent sessions", () => {
  const state = new AppState();
  state.loadIntegrationTasks();
  state.sessions.apply(event("claude-session", { eventType: "toolStarted" }), 1);
  state.syncAgentTasks();
  state.toggleIntegration("integration_resend");
  assert.equal(state.focusTask.sessionId, "claude-session");
  assert.equal(state.focusTask.state, "working");
  assert.equal(state.tasks.length, 7);
  assert.ok(state.tasks.find((task) => task.id === "integration_codex"));
});

test("focusing a group does not dismiss another session's approval badge", () => {
  const state = new AppState();
  state.loadIntegrationTasks();
  state.sessions.apply(event("visible"), 1);
  state.syncAgentTasks();
  state.sessions.apply(event("waiting", { eventType: "approvalRequested", requiresApproval: true }), 2);
  state.syncAgentTasks();
  state.setFocus("integration_claude");
  assert.equal(state.focusTask.sessionId, "visible");
  assert.equal(state.focusTask.pillBadge, "approval");
});
