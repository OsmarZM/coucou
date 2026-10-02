// Coucou owns these CLI conversations. External terminal sessions are never
// attached implicitly. The reducer only accepts events for the current turn.
import { Bridge, onEvent, type AgentChatEvent, type AgentChatStart, type AgentChatStatus, type ChatAgent } from "./bridge";
import { tr } from "./i18n";
import type { AgentUsage, AgentActivity, ContextReference, SessionRecord, StoredMessage, PersonalContextHistory } from "./personal-types";

export type ChatProvider = ChatAgent | "anthropic";
export const PERSONAL_CONTEXT_ID = "personal-main";
export interface AgentChatMessage { id: string; role: "user" | "assistant"; content: string; agent: ChatAgent; createdAt: number; contentTruncated?: boolean }
export interface TimelineMessage extends AgentChatMessage { conversationId: string }
export type ChatDecision = "allow" | "allowConversation" | "deny";
export interface ChatApproval { requestId: string; title: string; detail: string; choices: ChatDecision[]; submitting: boolean }
export interface AgentConversation {
  id: string;
  agent: ChatAgent;
  cwd: string;
  writable: boolean;
  sessionId: string | null;
  contextId: string | null;
  updatedAt: number;
  messages: AgentChatMessage[];
  tools: string[];
  runId: string | null;
  status: "idle" | "running" | "stopping" | "completed" | "interrupted" | "failed";
  approval: ChatApproval | null;
  error: string | null;
  draft: string;
  usage: AgentUsage | null;
  activities: Record<string, AgentActivity>;
  processCount: number | null;
  context: { memories: ContextReference[]; skills: ContextReference[]; truncated: boolean } | null;
  history: { persisted: boolean; reason: string | null } | null;
}
interface ConversationConfig { agent: ChatAgent; cwd: string; writable: boolean; sessionId?: string | null; contextId?: string | null }
interface MetadataStorage { getItem(key: string): string | null; setItem(key: string, value: string): void }
const AGENTS: ChatAgent[] = ["codex", "claude", "gemini", "copilot"];
const STORAGE_KEY = "coucou.cli-chat.metadata.v1";
const MAX_CONVERSATIONS = 16;
const MAX_MESSAGES = 100;
const MAX_VISIBLE_TEXT = 65_536;
const MAX_QUERY = 16_384;
let sequence = 0;
const id = () => globalThis.crypto?.randomUUID?.() ?? `coucou-${Date.now()}-${++sequence}-${Math.random().toString(36).slice(2)}`;
const bounded = (value: unknown, length: number) => typeof value === "string" ? value.slice(0, length) : "";
const emptyExtras = () => ({ draft: "", usage: null, activities: {}, processCount: null, context: null, history: null });
const validUsage = (value: Record<string, unknown>, runId: string): value is Record<string, unknown> & AgentUsage => {
  if (value.runId !== runId || typeof value.provider !== "string" || !Number.isFinite(value.capturedAt)) return false;
  const quota = value.quota as AgentUsage["quota"] | undefined;
  if (!quota || !["observed", "unavailable"].includes(quota.availability) || !Array.isArray(quota.windows) || quota.windows.length > 32) return false;
  const counter = (n: unknown) => n === null || (typeof n === "number" && Number.isFinite(n) && n >= 0);
  for (const token of [value.tokens, value.lastTurnTokens]) {
    if (token === null) continue;
    if (!token || typeof token !== "object") return false;
    const t = token as Record<string, unknown>;
    if (!["conversation", "turn", "model_call"].includes(String(t.scope)) || t.unit !== "tokens" || !["input", "output", "cachedInput", "cacheCreation", "reasoning", "total"].every((key) => counter(t[key]))) return false;
  }
  if (value.context !== null) {
    const context = value.context as AgentUsage["context"];
    if (!context || context.unit !== "tokens" || !counter(context.used) || !counter(context.capacity)) return false;
  }
  return quota.windows.every((window) => window && typeof window.id === "string" && window.unit === "percent" && ["available", "awaiting_update", "unavailable"].includes(window.state) && counter(window.usedPercent) && counter(window.remainingPercent) && (window.remainingPercent === null || window.remainingPercent <= 100));
};
/** Diagnostics are metadata; raw CLI errors may echo credentials or prompts. */
export function chatDiagnostic(value: unknown) {
  const clean = String(value ?? "").replace(/^Error:\s*/, "")
    .replace(/\u001b\[[0-?]*[ -/]*[@-~]/g, "")
    .replace(/\u001b\][^\u0007]*(?:\u0007|\u001b\\)/g, "")
    .replace(/[\u0000-\u001f\u007f]/g, " ").trim();
  const credential = /\b(?:bearer|basic)\s+[a-z0-9._~+/=-]{6,}|\b(?:sk-|ghp_|github_pat_)[a-z0-9_-]+|\b(?:password|passwd|pwd|secret|token|api[_ -]?key|access[_ -]?token|refresh[_ -]?token|client[_ -]?secret|database_url)["']?\s*[:=]\s*["']?\S|--(?:password|token|secret|api-key)(?:=|\s+)\S|\b[a-z][a-z0-9+.-]*:\/\/[^\s/:@]+:[^\s/@]+@/i;
  if (credential.test(clean)) return tr("chat.diagnosticOmitted");
  return clean.slice(0, 2000);
}

export class AgentChatStore {
  provider: ChatProvider = "codex";
  selectedId: string | null = null;
  selectionGeneration = 0;
  readonly conversations: AgentConversation[] = [];
  statuses: AgentChatStatus[] = [];
  statusError: string | null = null;
  personalDraft = "";
  private contextMessages: TimelineMessage[] = [];
  private contextGeneration = 0;
  private messageClock = 0;
  contextTruncated = false;
  private listeners = new Set<() => void>();
  private storage: MetadataStorage | null;

  constructor(storage: MetadataStorage | null = null) {
    this.storage = storage;
    this.restore();
  }
  subscribe(listener: () => void) { this.listeners.add(listener); return () => { this.listeners.delete(listener); }; }
  notify() { for (const listener of this.listeners) listener(); }
  get current() { return this.conversations.find((conversation) => conversation.id === this.selectedId) ?? null; }
  select(conversationId: string) {
    const conversation = this.conversations.find((entry) => entry.id === conversationId);
    if (!conversation) return false;
    this.selectedId = conversation.id;
    this.selectionGeneration++;
    this.provider = conversation.agent;
    this.persist();
    this.notify();
    return true;
  }
  setProvider(provider: ChatProvider) { this.selectionGeneration++; this.provider = provider; this.notify(); }
  /** Choosing a character selects a channel. It never starts or resumes a turn. */
  selectPersonal(agent: ChatAgent) {
    if (!AGENTS.includes(agent)) throw new Error(tr("chat.agentUnsupported"));
    const channel = this.conversations.filter((entry) => entry.agent === agent && entry.contextId === PERSONAL_CONTEXT_ID && !entry.cwd).sort((a, b) => b.updatedAt - a.updatedAt)[0];
    if (channel) { this.select(channel.id); return channel; }
    return this.create({ agent, cwd: "", writable: false, contextId: PERSONAL_CONTEXT_ID });
  }
  setPersonalDraft(draft: string) { this.personalDraft = draft.slice(0, MAX_QUERY); }
  timeline(contextId = PERSONAL_CONTEXT_ID): TimelineMessage[] {
    const messages = new Map<string, TimelineMessage>();
    if (contextId === PERSONAL_CONTEXT_ID) for (const entry of this.contextMessages) messages.set(`${entry.conversationId}:${entry.id}`, entry);
    for (const channel of this.conversations.filter((entry) => entry.contextId === contextId && !entry.cwd)) {
      for (const message of channel.messages) messages.set(`${channel.id}:${message.id}`, { ...message, conversationId: channel.id });
    }
    const ordered = [...messages.values()].sort((a, b) => a.createdAt - b.createdAt || a.id.localeCompare(b.id)).slice(-MAX_MESSAGES);
    let chars = 0;
    return ordered.reverse().filter((message) => { chars += message.content.length; return chars <= MAX_VISIBLE_TEXT; }).reverse();
  }
  applyContextHistory(history: PersonalContextHistory) {
    if (history.contextId !== PERSONAL_CONTEXT_ID || !Array.isArray(history.messages)) return false;
    this.contextMessages = history.messages.filter((entry) => AGENTS.includes(entry.agent) && ["user", "assistant"].includes(entry.role) && typeof entry.id === "string" && typeof entry.conversationId === "string" && typeof entry.content === "string" && Number.isFinite(entry.createdAt)).slice(-MAX_MESSAGES).map((entry) => ({ id: bounded(entry.id, 128), conversationId: bounded(entry.conversationId, 128), agent: entry.agent, role: entry.role as "user" | "assistant", content: bounded(entry.content, MAX_VISIBLE_TEXT), createdAt: entry.createdAt, contentTruncated: entry.contentTruncated === true }));
    this.contextTruncated = history.truncated === true;
    this.notify(); return true;
  }
  async refreshContextHistory() {
    const generation = ++this.contextGeneration;
    const [history, sessions] = await Promise.allSettled([Bridge.historyContext(PERSONAL_CONTEXT_ID), Bridge.historyList()]);
    if (generation !== this.contextGeneration) return;
    if (history.status === "fulfilled") this.applyContextHistory(history.value);
    else { this.statusError = chatDiagnostic(history.reason); this.notify(); }
    if (sessions.status === "fulfilled") this.adoptPersonalMetadata(sessions.value);
  }
  /** Binding comes from the backend; metadata alone never proves native ownership. */
  adoptPersonalMetadata(sessions: SessionRecord[]) {
    let changed = false;
    for (const channel of this.conversations) {
      if (channel.contextId || channel.cwd || channel.runId) continue;
      const matched = sessions.some((session) => session.contextId === PERSONAL_CONTEXT_ID && session.conversationId === channel.id && session.agent === channel.agent && !session.cwd && session.sessionId === channel.sessionId);
      if (matched) { channel.contextId = PERSONAL_CONTEXT_ID; changed = true; }
    }
    if (changed) { this.persist(); this.notify(); }
    return changed;
  }
  create(config: ConversationConfig) {
    if (!AGENTS.includes(config.agent)) throw new Error(tr("chat.agentUnsupported"));
    const cwd = config.cwd.trim();
    const sessionId = config.sessionId?.trim() || null;
    if (cwd.length > 4096 || (sessionId?.length ?? 0) > 512) throw new Error(tr("chat.configTooLong"));
    if (this.conversations.length >= MAX_CONVERSATIONS) {
      const oldest = this.conversations.filter((entry) => !entry.runId).sort((a, b) => a.updatedAt - b.updatedAt)[0];
      if (!oldest) throw new Error(tr("chat.stopBeforeNew"));
      this.conversations.splice(this.conversations.indexOf(oldest), 1);
    }
    const conversation: AgentConversation = {
      id: id(), agent: config.agent, cwd, writable: !!cwd && config.writable, sessionId,
      contextId: !cwd ? config.contextId !== undefined ? config.contextId : sessionId ? null : PERSONAL_CONTEXT_ID : null,
      updatedAt: Date.now(), messages: [], tools: [], runId: null, status: "idle", approval: null, error: null, ...emptyExtras(),
    };
    this.conversations.push(conversation);
    this.selectedId = conversation.id;
    this.selectionGeneration++;
    this.provider = conversation.agent;
    this.persist();
    this.notify();
    return conversation;
  }
  begin(conversationId: string, query: string, attachmentIds: string[] = []): AgentChatStart {
    const conversation = this.conversations.find((entry) => entry.id === conversationId);
    if (!conversation) throw new Error(tr("chat.chooseFirst"));
    if (conversation.runId) throw new Error(tr("chat.activeTurn"));
    if (attachmentIds.length > 10 || attachmentIds.some((entry) => typeof entry !== "string" || !entry || entry.length > 128) || new Set(attachmentIds).size !== attachmentIds.length) throw new Error(tr("chat.attachmentLimit"));
    query = query.trim();
    if (!query || query.length > MAX_QUERY) throw new Error(tr("chat.queryLimit"));
    const runId = id();
    conversation.runId = runId;
    conversation.status = "running";
    conversation.error = null;
    conversation.approval = null;
    conversation.tools = [];
    conversation.activities = {};
    conversation.context = null;
    conversation.history = null;
    conversation.updatedAt = Date.now();
    const createdAt = Math.max(Date.now(), this.messageClock + 1); this.messageClock = createdAt + 1;
    conversation.messages.push({ id: `${runId}-user`, role: "user", content: query, agent: conversation.agent, createdAt }, { id: `${runId}-assistant`, role: "assistant", content: "", agent: conversation.agent, createdAt: createdAt + 1 });
    this.limit(conversation);
    this.persist();
    this.notify();
    return { conversationId, runId, agent: conversation.agent, cwd: conversation.cwd, sessionId: conversation.sessionId, query, writable: conversation.writable, attachmentIds: [...attachmentIds], contextId: conversation.contextId };
  }
  failStart(conversationId: string, runId: string, error: string) {
    this.apply({ conversationId, runId, kind: "completed", data: { status: "failed", error } });
  }
  markStopping(conversationId: string, runId: string) {
    const conversation = this.conversations.find((entry) => entry.id === conversationId && entry.runId === runId);
    if (!conversation) return false;
    conversation.status = "stopping";
    conversation.approval = null;
    this.notify();
    return true;
  }
  cancelFailed(conversationId: string, runId: string, error: string) {
    const conversation = this.conversations.find((entry) => entry.id === conversationId && entry.runId === runId);
    if (!conversation) return;
    conversation.status = "running";
    conversation.error = chatDiagnostic(error);
    this.notify();
  }
  apply(event: AgentChatEvent) {
    const conversation = this.conversations.find((entry) => entry.id === event?.conversationId);
    if (!conversation || !event.runId || conversation.runId !== event.runId || !event.data || typeof event.data !== "object") return false;
    const data = event.data;
    const stopping = conversation.status === "stopping";
    switch (event.kind) {
      case "session": {
        if (typeof data.sessionId !== "string" || data.sessionId.length > 512) return false;
        const sessionId = bounded(data.sessionId, 512);
        if (!sessionId) return false;
        conversation.sessionId = sessionId;
        break;
      }
      case "delta": {
        if (stopping) return false;
        const message = conversation.messages.find((entry) => entry.id === `${event.runId}-assistant` || entry.id === event.runId);
        if (!message) return false;
        const text = bounded(data.text, MAX_VISIBLE_TEXT);
        message.content = (message.content + text).slice(0, MAX_VISIBLE_TEXT);
        this.limit(conversation);
        break;
      }
      case "message": {
        if (stopping) return false;
        const message = conversation.messages.find((entry) => entry.id === `${event.runId}-assistant` || entry.id === event.runId);
        if (!message) return false;
        message.content = bounded(data.text, MAX_VISIBLE_TEXT);
        this.limit(conversation);
        break;
      }
      case "status":
      case "tool":
        if (stopping) return false;
        conversation.tools.push(chatDiagnostic(data.text));
        conversation.tools = conversation.tools.slice(-8);
        break;
      case "approval": {
        if (stopping) return false;
        if (typeof data.requestId !== "string" || data.requestId.length > 512) return false;
        const requestId = bounded(data.requestId, 512);
        if (!requestId || !Array.isArray(data.choices) || !data.choices.includes("allow") || !data.choices.includes("deny")) return false;
        const detail = bounded(data.detail, 16_385);
        if (!detail.trim() || detail.length > 16_384) return false; // Never approve a silently shortened description.
        const choices = data.choices.filter((choice): choice is ChatDecision => ["allow", "allowConversation", "deny"].includes(String(choice)));
        conversation.approval = { requestId, title: bounded(data.title, 200), detail, choices: [...new Set(choices)], submitting: false };
        break;
      }
      case "usage":
        if (!validUsage(data, event.runId) || data.provider !== conversation.agent) return false;
        // Provider snapshots are cumulative or turn-scoped by their own DTO.
        // Replacing is essential: adding snapshots would double-count tokens.
        conversation.usage = structuredClone(data);
        break;
      case "activity": {
        if (Number.isInteger(data.processCount) && Number(data.processCount) >= 0) conversation.processCount = Math.min(64, Number(data.processCount));
        if (typeof data.id !== "string" || !data.id || data.id.length > 512) {
          if (conversation.processCount === null) return false;
          break;
        }
        if (stopping || data.runId !== event.runId || data.provider !== conversation.agent || !["tool", "subagent"].includes(String(data.kind)) || !["running", "pending", "completed", "failed", "unknown"].includes(String(data.status))) return false;
        const activityKey = `${data.kind}:${data.id}`;
        if (!(activityKey in conversation.activities) && Object.keys(conversation.activities).length >= 64) return false;
        conversation.activities[activityKey] = structuredClone(data) as unknown as AgentActivity;
        break;
      }
      case "context":
        if (!Array.isArray(data.memories) || !Array.isArray(data.skills)) return false;
        conversation.context = { memories: data.memories.slice(0, 100) as ContextReference[], skills: data.skills.slice(0, 100) as ContextReference[], truncated: data.truncated === true };
        break;
      case "history":
        if (typeof data.persisted !== "boolean") return false;
        conversation.history = { persisted: data.persisted, reason: data.reason == null ? null : chatDiagnostic(data.reason) };
        break;
      case "learning":
        if (typeof data.id !== "string" || !["pending", "approved", "rejected", "superseded"].includes(String(data.state)) || !Number.isSafeInteger(data.revision)) return false;
        break;
      case "attachments": break; // Document store consumes the same scoped event.
      case "approvalResolved":
        if (conversation.approval?.requestId !== data.requestId) return false;
        conversation.approval = null;
        break;
      case "error": conversation.error = chatDiagnostic(data.message); break;
      case "completed":
        if (!["completed", "interrupted", "failed"].includes(String(data.status))) return false;
        conversation.status = data.status as "completed" | "interrupted" | "failed";
        conversation.error = chatDiagnostic(data.error) || conversation.error;
        conversation.runId = null;
        conversation.approval = null;
        conversation.activities = {};
        conversation.processCount = 0;
        break;
      default: return false;
    }
    conversation.updatedAt = Date.now();
    // Disk/browser storage contains metadata only, never messages or approvals.
    if (event.kind === "session" || event.kind === "completed") this.persist();
    this.notify();
    return true;
  }
  restoreHistory(session: SessionRecord, messages: StoredMessage[]) {
    if (!AGENTS.includes(session.agent as ChatAgent) || !session.conversationId || session.conversationId.length > 128 || (session.cwd?.length ?? 0) > 4096 || (session.sessionId?.length ?? 0) > 512) throw new Error(tr("chat.invalidHistory"));
    let conversation = this.conversations.find((entry) => entry.id === session.conversationId);
    if (conversation?.runId) { this.select(conversation.id); return conversation; }
    if (!conversation) {
      conversation = this.create({ agent: session.agent as ChatAgent, cwd: session.cwd ?? "", writable: false, sessionId: session.sessionId, contextId: session.contextId ?? null });
      conversation.id = session.conversationId;
      this.selectedId = conversation.id;
    }
    conversation.messages = messages.filter((entry) => entry.conversationId === session.conversationId && ["user", "assistant"].includes(entry.role)).slice(-MAX_MESSAGES).map((entry) => ({ id: bounded(entry.id, 128), role: entry.role as "user" | "assistant", content: bounded(entry.content, MAX_VISIBLE_TEXT), agent: conversation!.agent, createdAt: Number.isFinite(entry.createdAt) ? entry.createdAt : 0 }));
    this.limit(conversation);
    this.select(conversation.id);
    return conversation;
  }
  forgetLocal(conversationId: string) {
    this.contextGeneration++;
    this.contextMessages = this.contextMessages.filter((entry) => entry.conversationId !== conversationId);
    const index = this.conversations.findIndex((entry) => entry.id === conversationId);
    if (index < 0) return;
    if (this.conversations[index].runId) throw new Error(tr("chat.stopBeforeForget"));
    this.conversations.splice(index, 1);
    if (this.selectedId === conversationId) this.selectedId = this.conversations.at(-1)?.id ?? null;
    this.persist(); this.notify();
  }
  private limit(conversation: AgentConversation) {
    while (conversation.messages.length > MAX_MESSAGES) conversation.messages.shift();
    let total = conversation.messages.reduce((length, message) => length + message.content.length, 0);
    while (total > MAX_VISIBLE_TEXT && conversation.messages.length > 2) {
      total -= conversation.messages.shift()!.content.length;
    }
    if (total > MAX_VISIBLE_TEXT) {
      const last = conversation.messages[conversation.messages.length - 1];
      last.content = last.content.slice(0, Math.max(0, MAX_VISIBLE_TEXT - (total - last.content.length)));
    }
  }
  private persist() {
    try {
      this.storage?.setItem(STORAGE_KEY, JSON.stringify({ selectedId: this.selectedId, conversations: this.conversations.map(({ id, agent, cwd, writable, sessionId, contextId, updatedAt }) => ({ id, agent, cwd, writable, sessionId, contextId, updatedAt })) }));
    } catch { /* Storage disabled or full; the live conversation remains usable. */ }
  }
  private restore() {
    try {
      const raw = this.storage?.getItem(STORAGE_KEY);
      if (!raw || raw.length > 100_000) return;
      const data = JSON.parse(raw) as Record<string, unknown>;
      if (!Array.isArray(data.conversations)) return;
      for (const entry of data.conversations.slice(-MAX_CONVERSATIONS)) {
        if (!entry || typeof entry !== "object" || !AGENTS.includes(entry.agent) || typeof entry.id !== "string" || entry.id.length > 128 || typeof entry.cwd !== "string" || entry.cwd.length > 4096 || (entry.sessionId != null && (typeof entry.sessionId !== "string" || entry.sessionId.length > 512)) || this.conversations.some((conversation) => conversation.id === entry.id)) continue;
        this.conversations.push({ id: entry.id, agent: entry.agent, cwd: entry.cwd, writable: !!entry.cwd.trim() && entry.writable === true, sessionId: entry.sessionId || null, contextId: !entry.cwd && entry.contextId === PERSONAL_CONTEXT_ID ? PERSONAL_CONTEXT_ID : null, updatedAt: Number(entry.updatedAt) || 0, messages: [], tools: [], runId: null, status: "idle", approval: null, error: null, ...emptyExtras() });
      }
      this.selectedId = typeof data.selectedId === "string" && this.conversations.some((entry) => entry.id === data.selectedId) ? data.selectedId : this.conversations[0]?.id ?? null;
      this.provider = this.current?.agent ?? "codex";
    } catch { /* Corrupt metadata does not prevent a fresh conversation. */ }
  }
}

let browserStorage: MetadataStorage | null = null;
try { if (typeof window !== "undefined") browserStorage = window.localStorage; } catch { /* Unavailable in private/sandboxed contexts. */ }
export const AgentChat = new AgentChatStore(browserStorage);
const eventObservers = new Set<(event: AgentChatEvent) => void>();
export function observeAgentChatEvents(observer: (event: AgentChatEvent) => void) { eventObservers.add(observer); return () => { eventObservers.delete(observer); }; }
export function handleAgentChatEvent(event: AgentChatEvent) {
  const accepted = AgentChat.apply(event);
  if (accepted) for (const observer of eventObservers) observer(event);
  return accepted;
}
export async function refreshAgentChatStatus() {
  try { AgentChat.statuses = (await Bridge.agentChatStatus()).map((status) => ({ ...status, detail: chatDiagnostic(status.detail) })); AgentChat.statusError = null; }
  catch (error) { AgentChat.statusError = chatDiagnostic(error); }
  AgentChat.notify();
}
export async function sendAgentChat(conversation: AgentConversation, query: string, attachmentIds: string[] = []) {
  const request = AgentChat.begin(conversation.id, query, attachmentIds);
  try { await Bridge.agentChatStart(request); return true; }
  catch (error) { AgentChat.failStart(request.conversationId, request.runId, chatDiagnostic(error)); return false; }
}
export async function cancelAgentChat(conversationId: string, runId: string) {
  if (!AgentChat.markStopping(conversationId, runId)) return false;
  try {
    const accepted = await Bridge.agentChatCancel(conversationId, runId);
    if (!accepted) AgentChat.cancelFailed(conversationId, runId, tr("chat.stopFailed"));
    return accepted;
  } catch (error) { AgentChat.cancelFailed(conversationId, runId, String(error)); return false; }
}
export async function cancelAgentChatTurns() {
  await Promise.allSettled(AgentChat.conversations.filter((entry) => entry.runId).map((entry) => cancelAgentChat(entry.id, entry.runId!)));
}
/** Hiding a permission card denies that request; ordinary turns keep running. */
export async function cancelAgentChatApprovals() {
  await Promise.allSettled(AgentChat.conversations.filter((entry) => entry.runId && entry.approval).map((entry) => denyAgentChatApproval(entry.id)));
}
export async function denyAgentChatApproval(conversationId: string) {
    const entry = AgentChat.conversations.find((conversation) => conversation.id === conversationId);
    if (!entry?.runId || !entry.approval) return;
    const runId = entry.runId!;
    const requestId = entry.approval!.requestId;
    // Invalidate the visible button before awaiting IPC.
    entry.approval = null;
    AgentChat.notify();
    try { await Bridge.agentChatDecide(entry.id, runId, requestId, "deny"); }
    catch (error) {
      if (entry.runId === runId) { entry.error = chatDiagnostic(error); AgentChat.notify(); }
    }
}
let approvalInteractionAllowed = () => false;
export function setApprovalInteractionGuard(guard: () => boolean) { approvalInteractionAllowed = guard; }
export async function decideAgentChat(conversationId: string, runId: string, requestId: string, decision: ChatDecision) {
  const conversation = AgentChat.current;
  if (!conversation || AgentChat.provider !== conversation.agent || conversation.id !== conversationId || conversation.runId !== runId || conversation.status !== "running" || conversation.approval?.requestId !== requestId || conversation.approval.submitting) return false;
  if (!conversation.approval.choices.includes(decision)) return false;
  if (decision !== "deny" && !approvalInteractionAllowed()) return false;
  conversation.approval.submitting = true;
  AgentChat.notify();
  try {
    if (decision !== "deny") {
      await Bridge.focusWindow(true);
      // Focus acquisition is asynchronous; navigation or expiry in that gap
      // must invalidate the exact click before it reaches the backend.
      if (!approvalInteractionAllowed() || AgentChat.current?.id !== conversationId || AgentChat.provider !== conversation.agent || conversation.runId !== runId || conversation.status !== "running" || conversation.approval?.requestId !== requestId) {
        if (conversation.runId === runId && conversation.approval?.requestId === requestId) {
          conversation.approval.submitting = false;
          AgentChat.notify();
        }
        return false;
      }
    }
    const accepted = await Bridge.agentChatDecide(conversationId, runId, requestId, decision);
    if (conversation.runId === runId && conversation.approval?.requestId === requestId) {
      conversation.approval = null;
      if (!accepted) conversation.error = tr("chat.expired");
      AgentChat.notify();
    }
    return accepted;
  } catch (error) {
    if (conversation.runId === runId && conversation.approval?.requestId === requestId) {
      conversation.approval.submitting = false;
      conversation.error = chatDiagnostic(error);
      AgentChat.notify();
    }
    return false;
  }
}
let unlisten: (() => void) | null = null;
let registration = 0;
export async function registerAgentChatHandlers(island?: { alert(view: "prompt"): void }) {
  const generation = ++registration;
  unlisten?.(); unlisten = null;
  const cleanup = await onEvent<AgentChatEvent>("agent-chat", (event) => {
    if (!handleAgentChatEvent(event)) return;
    if (event.kind === "approval" && AgentChat.provider !== "anthropic" && AgentChat.current?.id === event.conversationId) island?.alert("prompt");
    if (event.kind === "completed" && AgentChat.conversations.find((entry) => entry.id === event.conversationId)?.contextId === PERSONAL_CONTEXT_ID) void AgentChat.refreshContextHistory();
  });
  if (generation !== registration) { cleanup(); return; }
  unlisten = cleanup;
  void refreshAgentChatStatus();
  void AgentChat.refreshContextHistory();
}
export function disposeAgentChatHandlers() { registration++; unlisten?.(); unlisten = null; }
