// Bounded, in-memory agent sessions. Projects are display data, never identity.
import type { BotStateName } from "./layout";

export const AGENT_IDS = ["claude", "codex", "gemini", "copilot"] as const;
export type AgentId = typeof AGENT_IDS[number];
export const AGENT_META = {
  claude: { taskId: "integration_claude", name: "Claude", color: "#F5F6F8", source: "claudeCode" },
  codex: { taskId: "integration_codex", name: "Codex", color: "#10A37F", source: "codex" },
  gemini: { taskId: "integration_gemini", name: "Gemini", color: "#6C9AFF", source: "gemini" },
  copilot: { taskId: "integration_copilot", name: "GitHub Copilot", color: "#B898FF", source: "copilot" },
} as const;

const EVENT_TYPES = [
  "sessionStarted", "turnStarted", "toolStarted", "toolFinished", "toolFailed",
  "approvalRequested", "notification", "turnFinished", "turnFailed", "sessionEnded", "interrupted",
] as const;
export type AgentEventType = typeof EVENT_TYPES[number];

export interface AgentEvent {
  protocolVersion: 1;
  agent: AgentId;
  sessionId: string;
  eventType: AgentEventType;
  cwd: string;
  turnId?: string;
  toolName?: string;
  summary?: string;
  approvalTarget?: string;
  requiresApproval: boolean;
  requestId?: string;
}

export interface AgentSession {
  key: string;
  agent: AgentId;
  sessionId: string;
  cwd: string;
  projectName: string;
  turnId?: string;
  generation: number;
  state: BotStateName;
  steps: string[];
  pillBadge: "approval" | "finished" | "error" | null;
  firstSeen: number;
  lastSeen: number;
  ended: boolean;
}

export const SESSION_LIMIT = 48;
export const SESSION_TTL_MS = 24 * 60 * 60 * 1000;
export const SUMMARY_LIMIT = 160;
export const STEP_LIMIT = 20;

export function sessionKey(agent: AgentId, sessionId: string): string {
  return JSON.stringify([agent, sessionId]);
}

export function agentForTask(id: string): AgentId | null {
  return AGENT_IDS.find((agent) => AGENT_META[agent].taskId === id) ?? null;
}

export function projectName(cwd: string): string {
  return cwd.replace(/[\\/]+$/, "").split(/[\\/]/).at(-1) || "Session";
}

function brief(value: string, limit = SUMMARY_LIMIT): string {
  return value.replace(/[\u0000-\u001f\u007f]/g, " ").replace(/\s+/g, " ").trim().slice(0, limit);
}

/** Validate even IPC payloads so a malformed producer never merges identities. */
export function readAgentEvent(value: unknown): AgentEvent | null {
  if (!value || typeof value !== "object") return null;
  const event = value as Record<string, unknown>;
  const validId = (id: unknown) => typeof id === "string" && id.length <= 256 &&
    id.trim().length > 0 && id.trim() === id && !/[\u0000-\u001f\u007f]/.test(id);
  if (event.protocolVersion !== 1 || !AGENT_IDS.includes(event.agent as AgentId) ||
      !EVENT_TYPES.includes(event.eventType as AgentEventType) ||
      !validId(event.sessionId) || (event.turnId != null && !validId(event.turnId)) ||
      (event.requestId != null && !validId(event.requestId)) ||
      typeof event.cwd !== "string" || event.cwd.length > 4096 || /[\u0000-\u001f\u007f]/.test(event.cwd) ||
      (event.approvalTarget != null && (typeof event.approvalTarget !== "string" || event.approvalTarget.length > 800))) return null;
  const optional = (key: string, max: number) =>
    typeof event[key] === "string" ? brief(event[key] as string, max) || undefined : undefined;
  return {
    protocolVersion: 1, agent: event.agent as AgentId, sessionId: event.sessionId as string,
    eventType: event.eventType as AgentEventType, cwd: event.cwd,
    turnId: event.turnId as string | undefined, toolName: optional("toolName", 80),
    summary: optional("summary", SUMMARY_LIMIT), approvalTarget: event.approvalTarget as string | undefined,
    requestId: event.requestId as string | undefined, requiresApproval: event.agent === "claude" && event.requiresApproval === true,
  };
}

export class SessionStore {
  private records = new Map<string, AgentSession>();
  private nextGeneration = 0;

  get size(): number { return this.records.size; }
  get(key: string): AgentSession | undefined { return this.records.get(key); }
  list(agent?: AgentId): AgentSession[] {
    return [...this.records.values()].filter((s) => !agent || s.agent === agent)
      .sort((a, b) => b.lastSeen - a.lastSeen || b.generation - a.generation);
  }

  prune(now: number, protectedKey?: string): void {
    for (const [key, session] of this.records) {
      if (key !== protectedKey && now - session.lastSeen >= SESSION_TTL_MS) this.records.delete(key);
    }
    const oldest = this.list().reverse();
    for (const session of oldest) {
      if (this.records.size <= SESSION_LIMIT) break;
      if (session.key !== protectedKey) this.records.delete(session.key);
    }
  }

  apply(event: AgentEvent, now: number, protectedKey?: string): AgentSession | null {
    const key = sessionKey(event.agent, event.sessionId);
    let session = this.records.get(key);
    // A late tool/finish from the previous turn must never overwrite a new turn.
    if (session && event.turnId && session.turnId && event.turnId !== session.turnId &&
        event.eventType !== "turnStarted" && event.eventType !== "sessionStarted") return null;
    if (!session) {
      session = { key, agent: event.agent, sessionId: event.sessionId, cwd: "", projectName: "Session",
        generation: 0, state: "idle", steps: [], pillBadge: null, firstSeen: now, lastSeen: now, ended: false };
      this.records.set(key, session);
    }
    session.generation = ++this.nextGeneration;
    session.lastSeen = now;
    if (event.cwd) { session.cwd = event.cwd; session.projectName = projectName(event.cwd); }
    if (event.turnId) session.turnId = event.turnId;
    const append = (label?: string) => {
      if (!label) return;
      session.steps.push(brief(label));
      if (session.steps.length > STEP_LIMIT) session.steps.shift();
    };
    switch (event.eventType) {
      case "sessionStarted":
        session.ended = false;
        session.state = "idle";
        session.pillBadge = null;
        session.steps = [];
        session.turnId = event.turnId;
        break;
      case "turnStarted":
        session.ended = false;
        session.state = "thinking";
        session.steps = [];
        session.pillBadge = null;
        session.turnId = event.turnId;
        append(event.summary || "Turn started");
        break;
      case "toolStarted":
        session.state = "working";
        session.pillBadge = null;
        append(event.summary || event.toolName || "Tool started");
        break;
      case "toolFinished": session.state = "working"; break;
      case "toolFailed":
        session.state = "working";
        append(event.summary || `${event.toolName || "Tool"} failed`);
        break;
      case "approvalRequested":
        session.state = event.requiresApproval ? "approval" : "question";
        session.pillBadge = "approval";
        append(event.summary || "Action required in terminal");
        break;
      case "notification": append(event.summary); break;
      case "turnFinished":
        session.state = "finished";
        session.pillBadge = "finished";
        append(event.summary || "Turn finished");
        break;
      case "turnFailed":
        session.state = "error";
        session.pillBadge = "error";
        append(event.summary || "Turn failed");
        break;
      case "sessionEnded":
        session.state = "idle";
        session.pillBadge = null;
        session.ended = true;
        append("Session ended");
        break;
      case "interrupted":
        session.state = "idle";
        session.pillBadge = null;
        append(event.summary || "Interrupted");
        break;
    }
    this.prune(now, protectedKey);
    return session;
  }

  /** Timer affects this exact event generation only; it never invents completion. */
  clearBadge(key: string, generation: number): boolean {
    const session = this.records.get(key);
    if (!session || session.generation !== generation) return false;
    session.pillBadge = null;
    return true;
  }

  releaseApproval(key: string, generation: number): void {
    const session = this.records.get(key);
    if (!session || session.generation !== generation || session.state !== "approval") return;
    session.state = "question";
    session.pillBadge = "approval";
  }

  decisionSent(key: string, generation: number): void {
    const session = this.records.get(key);
    if (!session || session.generation !== generation) return;
    session.state = "working";
    session.pillBadge = null;
  }
}
