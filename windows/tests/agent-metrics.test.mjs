import test from "node:test";
import assert from "node:assert/strict";
import { aggregateAgentMetrics, countLabel, EXTERNAL_FRESH_SECONDS } from "../src/core/agent-metrics.ts";

const NOW = 10_000;
const token = (total, changes = {}) => ({ scope: "conversation", source: "thread/tokenUsage/updated", quality: "observed", capturedAt: NOW, unit: "tokens", input: null, output: null, cachedInput: null, cacheCreation: null, reasoning: null, total, ...changes });
const quota = (changes = {}) => ({ availability: "observed", accountScope: "acct:a", source: "account/rateLimits/read", capturedAt: NOW, windows: [{ id: "codex:primary", source: "account/rateLimits/read", capturedAt: NOW, quality: "observed", unit: "percent", usedPercent: 20, remainingPercent: 80, durationMinutes: 120, resetsAt: NOW + 100, state: "available" }], message: null, ...changes });
const usage = (total = 100, changes = {}) => ({ provider: "codex", sessionId: "native-a", runId: "run-a", sourceVersion: "0.159.2", capturedAt: NOW, tokens: token(total), lastTurnTokens: null, context: null, quota: quota(), ...changes });
const conversation = (changes = {}) => ({ id: "c-a", agent: "codex", sessionId: "native-a", runId: "run-a", status: "running", approval: null, updatedAt: NOW * 1000, usage: usage(), activities: {}, processCount: 2, ...changes });
const external = (changes = {}) => ({ agent: "codex", sessionId: "external-a", state: "working", lastSeen: NOW * 1000, ended: false, ...changes });
const activity = (changes = {}) => ({ provider: "codex", sessionId: "native-a", runId: "run-a", kind: "tool", id: "tool-a", status: "running", source: "item/started", capturedAt: NOW, quality: "observed", ...changes });
const metrics = (conversations = [], sessions = [], now = NOW) => aggregateAgentMetrics(conversations, sessions, now).codex;

test("native sessions and stale managed hooks are deduplicated across active and inactive conversations", () => {
  const result = metrics([conversation(), conversation({ id: "c-alias", runId: null, status: "completed", processCount: 0 })], [external({ sessionId: "native-a" }), external({ sessionId: "other" }), external({ sessionId: "other" })]);
  assert.equal(result.managedActive, 1);
  assert.equal(result.externalActive, 1);
  assert.equal(result.activeTotal, 2);
  const idle = metrics([conversation({ runId: null, status: "completed", processCount: 0 })], [external({ sessionId: "native-a" })]);
  assert.equal(idle.activeTotal, 0);
  assert.equal(idle.badge, "");
});

test("session identity includes provider and current approvals remain busy", () => {
  const values = aggregateAgentMetrics([conversation({ approval: { requestId: "decision" } })], [external({ agent: "claude", sessionId: "native-a", state: "question" })], NOW);
  assert.equal(values.codex.activeTotal, 1);
  assert.equal(values.codex.awaitingApproval, 1);
  assert.equal(values.claude.externalActive, 1);
  assert.equal(values.claude.awaitingApproval, 1);
});

test("external hook without a recent event becomes unknown rather than forever running or completed", () => {
  const stale = external({ lastSeen: (NOW - EXTERNAL_FRESH_SECONDS - 1) * 1000 });
  const result = metrics([], [stale]);
  assert.equal(result.externalActive, 0);
  assert.equal(result.externalUnknown, 1);
  assert.equal(result.sessions.total, null);
  assert.equal(result.badge, "?");
  assert.equal(result.processes.total, null);
  assert.equal(result.subagents.total, null);
});

test("native subagents do not become additional hook main sessions or OS processes", () => {
  const child = activity({ kind: "subagent", id: "native-child", source: "item/collabAgentToolCall.agentsStates" });
  const result = metrics([conversation({ activities: { one: child, duplicate: child, tool: activity() } })], [external({ sessionId: "native-child" })]);
  assert.equal(result.activeTotal, 1);
  assert.equal(result.subagents.observedCount, 1);
  assert.equal(result.tools.observedCount, 1);
  assert.equal(result.processes.total, 2);
  assert.equal(result.subagents.total, null); // event coverage is partial
});

test("the same native child referenced by two parent sessions is one subagent", () => {
  const child = activity({ kind: "subagent", id: "native-child", source: "item/collabAgentToolCall.agentsStates" });
  const secondChild = { ...child, sessionId: "native-b", runId: "run-b" };
  const result = metrics([conversation({ activities: { child } }), conversation({ id: "parent-b", sessionId: "native-b", runId: "run-b", activities: { child: secondChild } })]);
  assert.equal(result.managedActive, 2);
  assert.equal(result.subagents.observedCount, 1);
});

test("activity is bound to current provider/session/run and terminal snapshots close the same ID", () => {
  const result = metrics([conversation({ activities: {
    running: activity(), completed: activity({ status: "completed" }),
    old: activity({ id: "old", runId: "previous" }), foreign: activity({ id: "foreign", sessionId: "other" }),
    pending: activity({ kind: "subagent", id: "pending-child", status: "pending" }),
    unknown: activity({ kind: "subagent", id: "lost-child", status: "unknown" }),
  } })]);
  assert.equal(result.tools.observedCount, 0);
  assert.equal(result.subagents.observedCount, 1);
  assert.equal(result.subagents.unknownCount, 1);
  assert.equal(countLabel(result.tools), "Indisponível");
});

test("Job process counts are summed once per managed run, with external or absent trees explicitly unknown", () => {
  const first = conversation();
  const second = conversation({ id: "c-b", sessionId: "native-b", runId: "run-b", processCount: 3 });
  assert.equal(metrics([first, first, second]).processes.total, 5);
  const partial = metrics([first, { ...second, processCount: null }], [external()]);
  assert.equal(partial.processes.observedCount, 2);
  assert.equal(partial.processes.total, null);
  assert.equal(partial.processes.unknownCount, 2);
  assert.equal(countLabel(partial.processes), "≥ 2 observados");
});

test("cumulative resume and duplicate native sessions replace token totals instead of adding them", () => {
  const older = conversation({ id: "old", updatedAt: (NOW - 10) * 1000, runId: null, usage: usage(100, { capturedAt: NOW - 10 }) });
  const resumed = conversation({ usage: usage(150) });
  const independent = conversation({ id: "c-b", sessionId: "native-b", runId: null, status: "completed", usage: usage(25, { sessionId: "native-b" }) });
  const result = metrics([older, resumed, independent]);
  assert.equal(result.tokens.total, 175);
  assert.equal(result.tokens.conversations.length, 2);
  assert.equal(result.tokens.conversations.find((item) => item.sessionId === "native-a").conversationIds.length, 2);
});

test("turn/model-call token metrics, missing values and external sessions are never converted into conversation totals", () => {
  const turn = conversation({ usage: usage(null, { tokens: null, lastTurnTokens: token(20, { scope: "turn", source: "ACP/session/prompt.usage" }) }) });
  assert.equal(metrics([turn]).tokens.total, null);
  assert.equal(metrics([turn]).tokens.conversations[0].scope, "turn");
  assert.equal(metrics([conversation()], [external()]).tokens.total, null);
  assert.equal(metrics([conversation({ usage: usage(null) })]).tokens.total, null);
  assert.equal(metrics([]).tokens.total, null);
});

test("newest active account invalidates older provider quotas and missing current snapshot does not reuse them", () => {
  const old = conversation({ id: "old", updatedAt: (NOW - 20) * 1000, sessionId: "native-old", runId: null, status: "completed", usage: usage(100, { sessionId: "native-old", capturedAt: NOW - 20 }) });
  const current = conversation({ usage: usage(100, { quota: quota({ accountScope: "acct:b", windows: [] }) }) });
  assert.equal(metrics([old, current]).quota.accountScope, "acct:b");
  assert.deepEqual(metrics([old, current]).quota.windows, []);
  assert.equal(metrics([old, conversation({ runId: "new-run" })]).quota, null);
});

test("expired reset does not imply plan recovery; null account scopes cannot merge concurrent quotas", () => {
  const expired = conversation({ usage: usage(100, { quota: quota({ windows: quota().windows.map((window) => ({ ...window, resetsAt: NOW - 1 })) }) }) });
  assert.equal(metrics([expired]).quota.windows[0].state, "awaiting_update");
  assert.equal(metrics([expired]).quota.windows[0].remainingPercent, null);
  const anonymous = conversation({ usage: usage(100, { quota: quota({ accountScope: null }) }) });
  const other = conversation({ id: "other", sessionId: "native-b", runId: "run-b", updatedAt: (NOW - 1) * 1000, usage: usage(100, { sessionId: "native-b", runId: "run-b" }) });
  assert.equal(metrics([anonymous, other]).quota, null);
  assert.equal(metrics([anonymous]).quota.quality, "partial");
});

test("no current monitored sessions has measured zero processes, while absent usage remains unavailable", () => {
  const result = metrics([conversation({ runId: null, status: "completed", processCount: 0, usage: null })]);
  assert.equal(result.sessions.total, 0);
  assert.equal(result.processes.total, 0);
  assert.equal(result.tools.total, 0);
  assert.equal(result.tokens.total, null);
  assert.equal(result.quota, null);
});
