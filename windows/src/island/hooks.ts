// Normalized hook events from the local relay, correlated by agent and session.
import { Bridge, onEvent } from "../core/bridge";
import { Sound } from "../core/sound";
import { State } from "../core/state";
import { AGENT_META, readAgentEvent, sessionKey, type AgentEvent } from "../core/sessions";
import type { Island } from "./island";

let pendingTimeout: number | null = null;
const badgeTimers = new Map<string, number>();
let pruneTimer: number | null = null;
const listeners: (() => void)[] = [];

/** A native prompt resumes whenever the card can no longer accept a decision. */
export function cancelPendingApproval(island: Island, decline = true) {
  if (pendingTimeout != null) window.clearTimeout(pendingTimeout);
  pendingTimeout = null;
  const pending = State.pendingApproval;
  if (!pending) return;
  State.pendingApproval = null;
  if (decline) void Bridge.approvalDecline(pending.requestId);
  State.sessions.releaseApproval(pending.sessionKey, pending.generation);
  State.syncAgentTasks();
  State.isPinned = false;
  island.dropPin();
  if (State.view === "approval") island.setView(State.defaultView());
  State.notify();
}

export function disposeHookHandlers(island: Island) {
  cancelPendingApproval(island);
  for (const timer of badgeTimers.values()) window.clearTimeout(timer);
  badgeTimers.clear();
  if (pruneTimer != null) window.clearInterval(pruneTimer);
  pruneTimer = null;
  for (const unlisten of listeners.splice(0)) unlisten();
}

export function registerHookHandlers(island: Island) {
  void onEvent<unknown>("agent-hook", (payload) => {
    const event = readAgentEvent(payload);
    if (event) handleAgentEvent(island, event);
  }).then((unlisten) => listeners.push(unlisten));
  void onEvent<{ requestId: string }>("approval-expired", ({ requestId }) => {
    if (State.pendingApproval?.requestId === requestId) cancelPendingApproval(island, false);
  }).then((unlisten) => listeners.push(unlisten));
  pruneTimer = window.setInterval(() => {
    State.sessions.prune(Date.now(), State.pendingApproval?.sessionKey);
    State.syncAgentTasks();
    pruneBadgeTimers();
    State.notify();
  }, 60_000);
  window.addEventListener("beforeunload", () => disposeHookHandlers(island), { once: true });
}

function pruneBadgeTimers() {
  for (const [key, timer] of badgeTimers) {
    if (!State.sessions.get(key)) {
      window.clearTimeout(timer);
      badgeTimers.delete(key);
    }
  }
}

export function handleAgentEvent(island: Island, event: AgentEvent) {
  if (State.paused) {
    if (event.requestId) void Bridge.approvalDecline(event.requestId);
    return;
  }
  const key = sessionKey(event.agent, event.sessionId);
  const pending = State.pendingApproval;
  if (event.eventType === "approvalRequested" && pending?.sessionKey === key && pending.requestId !== event.requestId) {
    if (event.requestId) void Bridge.approvalDecline(event.requestId);
    return;
  }
  const session = State.sessions.apply(event, Date.now(), State.pendingApproval?.sessionKey);
  if (!session) {
    if (event.requestId) void Bridge.approvalDecline(event.requestId);
    return;
  }
  if (pending?.sessionKey === key && ["sessionEnded", "turnFinished", "turnFailed", "interrupted", "turnStarted"].includes(event.eventType)) {
    cancelPendingApproval(island);
  }
  if (State.pendingApproval?.sessionKey === key && session.state === "approval") {
    State.pendingApproval.generation = session.generation;
  }
  const taskId = AGENT_META[event.agent].taskId;
  const info = State.integrations[taskId] ?? { data: {}, error: null, loaded: false, configured: false };
  State.integrations[taskId] = { ...info, loaded: true, data: { ...info.data, lastEventAt: session.lastSeen } };
  State.syncAgentTasks();
  const focused = State.focusId === taskId && State.selectedSessions[event.agent] === key;
  const previousTimer = badgeTimers.get(key);
  if (previousTimer != null) {
    window.clearTimeout(previousTimer);
    badgeTimers.delete(key);
  }

  switch (event.eventType) {
    case "sessionStarted": Sound.play("work"); island.reveal(); break;
    case "turnStarted":
    case "toolStarted": island.reveal(); break;
    case "turnFinished": {
      Sound.play("finish");
      if (focused && !State.pendingApproval) island.alert("finished");
      else island.reveal();
      const generation = session.generation;
      badgeTimers.set(key, window.setTimeout(() => {
        badgeTimers.delete(key);
        if (State.sessions.clearBadge(key, generation)) {
          State.syncAgentTasks();
          State.notify();
        }
      }, 5200));
      break;
    }
    case "turnFailed":
      Sound.play("error");
      if (focused && !State.pendingApproval) island.alert("error");
      else island.reveal();
      break;
    case "approvalRequested": {
      // A badge is not a card. An unfocused session returns to its native prompt.
      const actionable = event.agent === "claude" && event.requiresApproval && event.requestId && focused &&
        (!State.pendingApproval || State.pendingApproval.requestId === event.requestId);
      if (!actionable) {
        if (event.requestId) void Bridge.approvalDecline(event.requestId);
        State.sessions.releaseApproval(key, session.generation);
        if (focused && !State.pendingApproval) island.alert("question");
        else island.reveal();
        break;
      }
      if (pendingTimeout != null) window.clearTimeout(pendingTimeout);
      const requestId = event.requestId!;
      State.pendingApproval = {
        requestId, taskId, sessionId: event.sessionId, sessionKey: key, generation: session.generation,
        tool: event.toolName || "Tool", command: event.approvalTarget || event.toolName || "Tool",
      };
      State.isPinned = true;
      island.alert("approval");
      Sound.play("approval");
      // The island's queued frame paints the card before acknowledgement. If the
      // user navigated away meanwhile, the short relay wait falls back natively.
      requestAnimationFrame(() => {
        if (State.pendingApproval?.requestId !== requestId) return;
        if (State.mode === "expanded" && State.view === "approval" && State.focusId === taskId && State.focusTask?.sessionKey === key) {
          void Bridge.approvalAck(requestId);
        } else cancelPendingApproval(island);
      });
      pendingTimeout = window.setTimeout(() => {
        if (State.pendingApproval?.requestId === requestId) cancelPendingApproval(island);
      }, 108_000);
      break;
    }
    default: break;
  }
  State.syncAgentTasks();
  pruneBadgeTimers();
  State.notify();
}
