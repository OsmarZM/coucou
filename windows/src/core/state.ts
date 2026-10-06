// App state — mirror of AppState.swift (the parts the island needs).

import type { BotEmoteName, BotStateName, IslandMode, IslandViewName } from "./layout";
import type { EyeShape } from "../mochi/engine";
import { AGENT_IDS, AGENT_META, agentForTask, SessionStore, type AgentId } from "./sessions";
import { AgentChat } from "./agent-chat";
import type { VisibilityMode } from "../island/fsm";

export type AgentSource = "claudeCode" | "codex" | "gemini" | "copilot" | "n8n";
export type PillBadge = "approval" | "finished" | "error";

export interface AgentTask {
  id: string;
  name: string;
  color: string;
  state: BotStateName;
  stepIndex: number;
  steps: string[];
  source: AgentSource;
  isIntegration: boolean;
  emote?: BotEmoteName | null;
  miniEye?: EyeShape | null;
  pillBadge?: PillBadge | null;
  sessionCwd?: string | null;
  sessionId?: string | null;
  sessionKey?: string | null;
  projectName?: string | null;
}

export interface ApprovalInfo {
  requestId: string;
  sessionId: string;
  tool: string;
  command: string;
  taskId: string;
  sessionKey: string;
  generation: number;
}

export interface ChatMessage {
  id: number;
  role: "user" | "assistant";
  content: string;
}

export type PromptContext =
  | { kind: "window"; appName: string; title: string; url?: string }
  | { kind: "file"; name: string; path?: string };

export interface ResultItem {
  label: string;
  detail: string;
  url?: string;
}

export interface SearchResult {
  title: string;
  items: ResultItem[];
  note?: string;
}

const task = (
  id: string, name: string, color: string, source: AgentSource,
): AgentTask => ({
  id, name, color, state: "idle", stepIndex: 0, steps: [], source, isIntegration: true,
});

/** Agent groups remain fixed; sessions never create unbounded UI pills. */
export const INTEGRATION_AGENTS: AgentTask[] = [
  ...AGENT_IDS.map((agent) => {
    const meta = AGENT_META[agent];
    return task(meta.taskId, meta.name, meta.color, meta.source);
  }),
  task("integration_resend", "Resend", "#22C55E", "n8n"),
  task("integration_n8n", "n8n", "#F29B38", "n8n"),
  task("integration_vercel", "Vercel", "#7C5CFF", "n8n"),
  task("integration_github", "GitHub", "#F4505E", "n8n"),
  task("integration_notion", "Notion", "#8C8C8C", "n8n"),
  task("integration_calcom", "Cal.com", "#C9956A", "n8n"),
  task("integration_stripe", "Stripe", "#0570DE", "n8n"),
];

export const TOGGLEABLE_INTEGRATION_IDS = [
  "integration_resend", "integration_n8n", "integration_vercel", "integration_github",
  "integration_notion", "integration_calcom", "integration_stripe",
];

/** What an integration poller last reported. */
export interface IntegrationInfo {
  data: Record<string, unknown>;
  error: string | null;
  loaded: boolean;
  configured: boolean;
}

export interface Settings {
  soundEnabled: boolean;
  soundVolume: number;
  autoCloseInterval: number;
  absenceInterval: number;
  visibilityMode: VisibilityMode;
  userPinned: boolean;
  activeIntegrations: string[];
  screen: "primary" | "cursor";
  autostart: boolean;
  hooksInstalled: boolean;
  /** Claude model used by the chat. */
  model: string;
}

export const DEFAULT_SETTINGS: Settings = {
  soundEnabled: true,
  soundVolume: 0.12,
  autoCloseInterval: 15,
  absenceInterval: 180,
  visibilityMode: "always",
  userPinned: false,
  activeIntegrations: [
    "integration_resend", "integration_n8n", "integration_vercel", "integration_github",
  ],
  screen: "primary",
  autostart: false,
  hooksInstalled: false,
  model: "claude-opus-5",
};

type Listener = () => void;

export class AppState {
  appVersion = "";
  private currentMode: IslandMode = "hidden";
  get mode() { return this.currentMode; }
  set mode(mode: IslandMode) { if (this.currentMode !== mode) { this.currentMode = mode; this.navigationGeneration++; } }
  private currentView: IslandViewName = "overview";
  navigationGeneration = 0;
  get view() { return this.currentView; }
  set view(view: IslandViewName) { if (this.currentView !== view) { this.currentView = view; this.navigationGeneration++; } }

  tasks: AgentTask[] = [];
  focusId: string | null = null;

  stateOverride: BotStateName | null = null;

  /** Cursor in logical screen pixels, origin top-left (like AppState.mousePosition). */
  mouse = { x: 0, y: 0 };
  /** Cursor relative to the island's top-left corner. */
  mouseInIsland = { x: 0, y: 0 };

  isPinned = false;
  /** Document preparation can retain the panel independently of drag animations. */
  attachmentBusy = false;
  paused = false;

  uploadProgress = 0;
  uploadDuration = 2.4;
  fileDragOver = false;

  promptContext: PromptContext | null = null;
  droppedFile: { name: string; path: string } | null = null;
  noteMessage: string | null = null;
  searchResult: SearchResult | null = null;
  chatHistory: ChatMessage[] = [];
  readonly agentChat = AgentChat;
  pendingApproval: ApprovalInfo | null = null;
  readonly sessions = new SessionStore();
  readonly selectedSessions: Partial<Record<AgentId, string>> = {};

  integrations: Record<string, IntegrationInfo> = {};

  lastActivity = performance.now();

  settings: Settings = { ...DEFAULT_SETTINGS };

  private listeners = new Set<Listener>();

  constructor() {
    this.agentChat.subscribe(() => this.notify());
  }

  subscribe(fn: Listener): () => void {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  }

  /** Marks the UI dirty; the island re-renders on the next frame. */
  notify() {
    for (const fn of this.listeners) fn();
  }

  get focusTask(): AgentTask | null {
    return this.tasks.find((t) => t.id === this.focusId) ?? this.tasks[0] ?? null;
  }

  get effectiveState(): BotStateName {
    if (this.stateOverride == null && this.view === "prompt" && this.agentChat.provider !== "anthropic") {
      const conversation = this.agentChat.current;
      if (conversation?.approval) return "approval";
      if (conversation?.runId) return "thinking";
    }
    return this.stateOverride ?? this.focusTask?.state ?? "idle";
  }

  get otherTasks(): AgentTask[] {
    return this.tasks.filter((t) => t.id !== this.focusId);
  }

  setFocus(id: string) {
    const t = this.tasks.find((x) => x.id === id);
    if (!t) return;
    this.focusId = id;
    const agent = agentForTask(id);
    const selected = agent ? this.sessions.get(this.selectedSessions[agent] ?? "") : undefined;
    if (selected && selected.state !== "approval" && selected.state !== "question") selected.pillBadge = null;
    if (!agent) t.pillBadge = null;
    this.syncAgentTasks();
    this.notify();
  }

  selectSession(agent: AgentId, key: string) {
    if (this.sessions.get(key)?.agent !== agent) return;
    this.selectedSessions[agent] = key;
    this.syncAgentTasks();
    this.notify();
  }

  /** Refresh each group's selected session, keeping unrelated service tasks intact. */
  syncAgentTasks() {
    for (const agent of AGENT_IDS) {
      const group = this.tasks.find((t) => t.id === AGENT_META[agent].taskId);
      if (!group) continue;
      const sessions = this.sessions.list(agent);
      const selected = this.sessions.get(this.selectedSessions[agent] ?? "") ?? sessions[0];
      if (selected) this.selectedSessions[agent] = selected.key;
      else delete this.selectedSessions[agent];
      group.state = selected?.state ?? "idle";
      group.steps = selected?.steps ?? [];
      group.stepIndex = Math.max(0, group.steps.length - 1);
      group.sessionCwd = selected?.cwd ?? null;
      group.sessionId = selected?.sessionId ?? null;
      group.sessionKey = selected?.key ?? null;
      group.projectName = selected?.projectName ?? null;
      group.pillBadge = sessions.some((s) => s.pillBadge === "approval") ? "approval"
        : sessions.some((s) => s.pillBadge === "error") ? "error"
        : sessions.some((s) => s.pillBadge === "finished") ? "finished" : null;
    }
  }

  updateTask(id: string, state: BotStateName) {
    const t = this.tasks.find((x) => x.id === id);
    if (!t) return;
    t.state = state;
    this.notify();
  }

  appendStep(id: string, step: string) {
    const t = this.tasks.find((x) => x.id === id);
    if (!t) return;
    t.steps.push(step);
    if (t.steps.length > 20) t.steps.shift();
    t.stepIndex = t.steps.length - 1;
    this.notify();
  }

  setPillBadge(id: string, badge: PillBadge | null) {
    const t = this.tasks.find((x) => x.id === id);
    if (!t) return;
    t.pillBadge = badge;
    this.notify();
  }

  /** Four agent groups always present; the max-four limit applies to services. */
  loadIntegrationTasks() {
    for (const proto of INTEGRATION_AGENTS) {
      const shouldLoad =
        agentForTask(proto.id) != null || this.settings.activeIntegrations.includes(proto.id);
      const idx = this.tasks.findIndex((t) => t.id === proto.id);
      if (shouldLoad && idx < 0) this.tasks.push({ ...proto, steps: [] });
      if (!shouldLoad && idx >= 0) this.tasks.splice(idx, 1);
    }
    // Keep the declared order so pills never shuffle.
    const order = INTEGRATION_AGENTS.map((t) => t.id);
    this.tasks.sort((a, b) => order.indexOf(a.id) - order.indexOf(b.id));
    if (!this.focusId) this.focusId = "integration_claude";
    this.syncAgentTasks();
    this.notify();
  }

  toggleIntegration(id: string) {
    if (!TOGGLEABLE_INTEGRATION_IDS.includes(id)) return;
    const active = this.settings.activeIntegrations;
    if (active.includes(id)) {
      this.settings.activeIntegrations = active.filter((x) => x !== id);
      if (this.focusId === id) this.focusId = "integration_claude";
    } else {
      if (active.length >= 4) return;
      this.settings.activeIntegrations = [...active, id];
    }
    this.loadIntegrationTasks();
  }

  defaultView(): IslandViewName {
    return this.tasks.length === 0 ? "empty" : "overview";
  }
}

export const State = new AppState();
