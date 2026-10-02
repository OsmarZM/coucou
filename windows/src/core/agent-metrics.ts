// Pure aggregation over observed records. A CLI process, a tool call and a
// native subagent have different identities and must not become extra sessions.
import { AGENT_IDS, type AgentId, type AgentSession } from "./sessions";
import type { AgentConversation } from "./agent-chat";
import type { AgentActivity, AgentUsage, RateWindow, TokenUsage } from "./personal-types";

export const EXTERNAL_FRESH_SECONDS = 180;
type Conversation = Pick<AgentConversation, "id" | "agent" | "sessionId" | "runId" | "status" | "approval" | "updatedAt" | "usage" | "activities" | "processCount">;
type External = Pick<AgentSession, "agent" | "sessionId" | "state" | "lastSeen" | "ended">;
export interface CountMetric {
  observedCount: number;
  total: number | null;
  unknownCount: number;
  quality: "observed" | "partial" | "unavailable";
  capturedAt: number | null;
  basis: string;
}
export interface ConversationTokens {
  conversationId: string;
  conversationIds: string[];
  sessionId: string | null;
  scope: TokenUsage["scope"] | null;
  total: number | null;
  source: string | null;
  capturedAt: number | null;
  quality: "observed" | "partial" | "unavailable";
}
export interface TokenMetric {
  total: number | null;
  quality: "observed" | "partial" | "unavailable";
  capturedAt: number | null;
  basis: string;
  conversations: ConversationTokens[];
}
export type ProviderQuota = AgentUsage["quota"] & {
  sessionId: string | null;
  runId: string;
  sourceVersion: string | null;
  quality: "observed" | "partial" | "unavailable";
  historical: boolean;
};
export interface AgentMetrics {
  provider: AgentId;
  managedActive: number;
  externalActive: number;
  externalUnknown: number;
  activeTotal: number;
  awaitingApproval: number;
  sessions: CountMetric;
  subagents: CountMetric;
  tools: CountMetric;
  processes: CountMetric;
  tokens: TokenMetric;
  quota: ProviderQuota | null;
  quotaReason: string | null;
  capturedAt: number;
  badge: string;
}

const key = (...parts: string[]) => JSON.stringify(parts);
const nativeKey = (provider: string, session: string) => key(provider, session);
const finiteCounter = (value: unknown): value is number => typeof value === "number" && Number.isSafeInteger(value) && value >= 0;
const validId = (value: unknown): value is string => typeof value === "string" && value.length > 0 && value.length <= 512 && !/[\u0000-\u001f\u007f]/.test(value);
const active = (conversation: Conversation) => validId(conversation.runId) && ["running", "stopping"].includes(conversation.status);
const unix = (milliseconds: number) => Number.isFinite(milliseconds) && milliseconds >= 0 ? milliseconds / 1000 : 0;
const fresh = (timestamp: number, now: number) => Number.isFinite(timestamp) && timestamp > 0 && timestamp <= now + 30 && now - timestamp <= EXTERNAL_FRESH_SECONDS;
const maxTime = (values: (number | null)[]) => values.filter((value): value is number => value !== null && Number.isFinite(value) && value > 0).reduce<number | null>((result, value) => result === null ? value : Math.max(result, value), null);
const observedUsage = (conversation: Conversation) => conversation.usage?.provider === conversation.agent && conversation.usage.sessionId === conversation.sessionId ? conversation.usage : null;
const newFirst = (left: Conversation, right: Conversation) => right.updatedAt - left.updatedAt || left.id.localeCompare(right.id);

function count(observedCount: number, unknownCount: number, complete: boolean, capturedAt: number | null, basis: string): CountMetric {
  return { observedCount, total: complete ? observedCount : null, unknownCount, quality: complete ? "observed" : observedCount ? "partial" : "unavailable", capturedAt, basis };
}

function normalizedWindows(windows: RateWindow[], now: number): RateWindow[] {
  return windows.map((window) => {
    const stale = window.state === "awaiting_update" || (window.resetsAt !== null && window.resetsAt <= now);
    return { ...window, state: stale ? "awaiting_update" : window.state, remainingPercent: stale ? null : window.remainingPercent };
  });
}

function quotaFor(conversations: Conversation[], now: number): { quota: ProviderQuota | null; quotaReason: string | null } {
  const running = conversations.filter(active).sort((left, right) => (observedUsage(right)?.capturedAt ?? 0) - (observedUsage(left)?.capturedAt ?? 0) || newFirst(left, right));
  let chosen: Conversation | undefined;
  if (running.length) {
    // The newest active run defines the current account. An old run's cached
    // quota cannot stand in for a new login or a snapshot that is still absent.
    if (running.some((conversation) => observedUsage(conversation)?.runId !== conversation.runId)) return { quota: null, quotaReason: "Aguardando uso e conta de uma sessão ativa; o saldo anterior não comprova a conta atual." };
    chosen = running[0];
  } else {
    chosen = conversations.filter((conversation) => observedUsage(conversation)).sort((left, right) => (observedUsage(right)?.capturedAt ?? 0) - (observedUsage(left)?.capturedAt ?? 0) || newFirst(left, right))[0];
  }
  if (!chosen) return { quota: null, quotaReason: "A CLI ainda não reportou cota da conta." };
  const usage = observedUsage(chosen)!;
  if (usage.quota.availability === "observed" && usage.quota.accountScope === null && running.length > 1) {
    return { quota: null, quotaReason: "Contas sem identidade verificável; consulte a cota de cada conversa." };
  }
  return {
    quota: { ...usage.quota, windows: normalizedWindows(usage.quota.windows, now), sessionId: usage.sessionId, runId: usage.runId, sourceVersion: usage.sourceVersion, quality: usage.quota.availability === "unavailable" ? "unavailable" : usage.quota.accountScope === null ? "partial" : "observed", historical: !running.length },
    quotaReason: usage.quota.availability === "unavailable" ? usage.quota.message : null,
  };
}

function tokensFor(groups: Conversation[][], externalActive: number, externalUnknown: number): TokenMetric {
  const details = groups.map((group): ConversationTokens => {
    const conversation = [...group].sort((left, right) => (observedUsage(right)?.capturedAt ?? 0) - (observedUsage(left)?.capturedAt ?? 0) || newFirst(left, right))[0];
    const usage = observedUsage(conversation);
    const token = usage?.tokens ?? usage?.lastTurnTokens;
    const valid = token?.unit === "tokens" && finiteCounter(token.total);
    return { conversationId: conversation.id, conversationIds: group.map((item) => item.id), sessionId: conversation.sessionId, scope: token?.scope ?? null, total: valid ? token.total : null, source: token?.source ?? null, capturedAt: token?.capturedAt ?? null, quality: valid ? token.quality : "unavailable" };
  });
  const compatible = details.length > 0 && externalActive === 0 && externalUnknown === 0 && details.every((item) => item.scope === "conversation" && item.quality === "observed" && item.total !== null && item.source === details[0].source);
  const sum = compatible ? details.reduce((sum, item) => sum + item.total!, 0) : null;
  const total = sum !== null && Number.isSafeInteger(sum) ? sum : null;
  return { total, quality: total !== null ? "observed" : details.some((item) => item.total !== null) ? "partial" : "unavailable", capturedAt: maxTime(details.map((item) => item.capturedAt)), basis: total !== null ? "Snapshots acumulados de conversas nativas distintas; cada sessão contada uma vez." : "Uso por conversa; escopos de turno/chamada, fontes diferentes ou sessões sem dados não são somados.", conversations: details };
}

/** Fresh hook states are observations, not an inventory of every CLI on the PC.
 * No hook/session is counted twice when its native ID belongs to any managed
 * conversation, even an inactive one. Old hook activity becomes unknown.
 */
export function aggregateAgentMetrics(conversations: readonly Conversation[], externalSessions: readonly External[], nowSeconds = Math.floor(Date.now() / 1000)): Record<AgentId, AgentMetrics> {
  const now = Number.isFinite(nowSeconds) && nowSeconds >= 0 ? nowSeconds : Math.floor(Date.now() / 1000);
  const managed = new Map<string, Conversation>();
  for (const conversation of conversations) {
    if (!AGENT_IDS.includes(conversation.agent) || !validId(conversation.id)) continue;
    const old = managed.get(conversation.id);
    if (!old || conversation.updatedAt >= old.updatedAt) managed.set(conversation.id, conversation);
  }
  const ownedNative = new Set<string>();
  const nativeChildren = new Set<string>();
  for (const conversation of managed.values()) {
    if (validId(conversation.sessionId)) ownedNative.add(nativeKey(conversation.agent, conversation.sessionId));
    for (const item of Object.values(conversation.activities)) {
      // Codex's agentsStates IDs are native threads. Claude task IDs are not
      // assumed to equal a hook session ID merely because their strings match.
      if (item.provider === "codex" && conversation.agent === "codex" && item.kind === "subagent" && item.source === "item/collabAgentToolCall.agentsStates" && validId(item.id) && item.sessionId === conversation.sessionId && item.runId === conversation.runId) nativeChildren.add(nativeKey("codex", item.id));
    }
  }
  return Object.fromEntries(AGENT_IDS.map((provider) => {
    const records = [...managed.values()].filter((conversation) => conversation.agent === provider);
    const groups = new Map<string, Conversation[]>();
    for (const conversation of records) {
      const identity = validId(conversation.sessionId) ? nativeKey(provider, conversation.sessionId) : key(provider, "conversation", conversation.id);
      groups.set(identity, [...(groups.get(identity) ?? []), conversation]);
    }
    const running = records.filter(active);
    const managedActive = [...groups.values()].filter((group) => group.some(active)).length;
    const external = new Map<string, External>();
    for (const session of externalSessions) {
      if (session.agent !== provider || !validId(session.sessionId)) continue;
      const identity = nativeKey(provider, session.sessionId);
      if (ownedNative.has(identity) || nativeChildren.has(identity)) continue;
      const old = external.get(identity);
      if (!old || session.lastSeen >= old.lastSeen) external.set(identity, session);
    }
    const externalBusy = [...external.values()].filter((session) => !session.ended && ["working", "thinking", "approval", "question"].includes(session.state));
    const currentExternal = externalBusy.filter((session) => fresh(unix(session.lastSeen), now));
    const externalActive = currentExternal.length;
    const externalUnknown = externalBusy.length - externalActive;
    const activity = new Map<string, AgentActivity>();
    for (const conversation of running) {
      for (const item of Object.values(conversation.activities)) {
        if (item.provider !== provider || item.runId !== conversation.runId || item.sessionId !== conversation.sessionId || !validId(item.id) || !["tool", "subagent"].includes(item.kind)) continue;
        const nativeChild = provider === "codex" && item.kind === "subagent" && item.source === "item/collabAgentToolCall.agentsStates";
        const identity = nativeChild ? key(provider, "native-subagent", item.id) : key(provider, item.sessionId ?? conversation.id, item.kind, item.id, item.kind === "tool" ? item.runId : "");
        const old = activity.get(identity);
        const terminal = (item: AgentActivity) => ["completed", "failed"].includes(item.status);
        if (!old || item.capturedAt > old.capturedAt || (item.capturedAt === old.capturedAt && terminal(item))) activity.set(identity, item);
      }
    }
    const activeItems = (kind: AgentActivity["kind"]) => [...activity.values()].filter((item) => item.kind === kind && ["running", "pending"].includes(item.status) && fresh(item.capturedAt, now));
    const unknownItems = (kind: AgentActivity["kind"]) => [...activity.values()].filter((item) => item.kind === kind && (item.status === "unknown" || (["running", "pending"].includes(item.status) && !fresh(item.capturedAt, now)))).length;
    const toolItems = activeItems("tool");
    const childItems = activeItems("subagent");
    const noActive = managedActive + externalActive + externalUnknown === 0;
    const processesObserved = running.reduce((sum, conversation) => sum + (finiteCounter(conversation.processCount) ? conversation.processCount : 0), 0);
    const processesUnknown = running.filter((conversation) => !finiteCounter(conversation.processCount)).length + externalActive + externalUnknown;
    const activeTotal = managedActive + externalActive;
    const awaitingApproval = [...groups.values()].filter((group) => group.some((conversation) => active(conversation) && conversation.approval !== null)).length + currentExternal.filter((session) => ["approval", "question"].includes(session.state)).length;
    const metric: AgentMetrics = {
      provider, managedActive, externalActive, externalUnknown, activeTotal, awaitingApproval,
      sessions: count(activeTotal, externalUnknown, externalUnknown === 0, maxTime([...running.map((item) => unix(item.updatedAt)), ...currentExternal.map((item) => unix(item.lastSeen))]), "Sessões principais nativas monitoradas; ganchos sem evento recente ficam desconhecidos após 180 s."),
      subagents: count(childItems.length, unknownItems("subagent") + externalActive + externalUnknown, noActive, maxTime(childItems.map((item) => item.capturedAt)), "IDs explícitos de subagentes nos eventos do turno; ausência de evento não comprova zero."),
      tools: count(toolItems.length, unknownItems("tool") + externalActive + externalUnknown, noActive, maxTime(toolItems.map((item) => item.capturedAt)), "Chamadas de ferramenta com ID e turno; sem saídas, comandos ou credenciais brutas."),
      processes: count(processesObserved, processesUnknown, processesUnknown === 0, maxTime(running.map((item) => unix(item.updatedAt))), "Processos observados nos Jobs do Windows. Sessões externas não têm árvore de PIDs atribuída."),
      tokens: tokensFor([...groups.values()], externalActive, externalUnknown),
      ...quotaFor(records, now), capturedAt: now,
      badge: activeTotal ? `${activeTotal}${externalUnknown ? "+?" : ""}` : externalUnknown ? "?" : "",
    };
    return [provider, metric];
  })) as Record<AgentId, AgentMetrics>;
}

export function countLabel(metric: CountMetric): string {
  if (metric.total !== null) return metric.total.toLocaleString("pt-BR");
  return metric.observedCount ? `≥ ${metric.observedCount.toLocaleString("pt-BR")} observados` : "Indisponível";
}
