import { h } from "./dom";
import { tr, type TextKey } from "../core/i18n";
import type { AgentConversation } from "../core/agent-chat";
import type { TokenUsage } from "../core/personal-types";
import { rateWindowTitle, reportedDuration } from "../core/usage-format";

export function observedNumber(value: number | null | undefined, suffix = "") {
  return typeof value === "number" && Number.isFinite(value) ? `${value.toLocaleString("pt-BR", { maximumFractionDigits: 2 })}${suffix}` : tr("usage.unavailable");
}
const time = (seconds: number | null | undefined) => typeof seconds === "number" && seconds > 0 ? new Date(seconds * 1000).toLocaleString("pt-BR") : tr("usage.unavailable");
const row = (label: string, value: string) => h("div", { class: "usage-row" }, h("span", { text: label }), h("span", { text: value }));
function tokenRows(tokens: TokenUsage, title: string) {
  const fields: [TextKey, number | null][] = [["usage.input", tokens.input], ["usage.output", tokens.output], ["usage.cache", tokens.cachedInput], ["usage.cacheCreation", tokens.cacheCreation], ["usage.reasoning", tokens.reasoning], ["usage.total", tokens.total]];
  return h("div", { class: "usage-block" }, h("strong", { text: title }), ...fields.map(([label, value]) => row(tr(label), observedNumber(value))), row(tr("usage.source"), tokens.source), row(tr("usage.captured"), time(tokens.capturedAt)), tokens.quality === "partial" ? h("span", { class: "chat-muted", text: tr("usage.partial") }) : null);
}
export function usagePanel(conversation: AgentConversation) {
  const usage = conversation.usage;
  const activities = Object.values(conversation.activities);
  const tools = activities.filter((item) => item.kind === "tool" && ["running", "pending"].includes(item.status)).length;
  const subagents = activities.filter((item) => item.kind === "subagent" && ["running", "pending"].includes(item.status)).length;
  const panel = h("div", { class: "usage-panel" }, row(tr("usage.processes"), observedNumber(conversation.processCount)), row(tr("usage.tools"), activities.length ? observedNumber(tools) : tr("usage.unavailable")), row(tr("usage.subagents"), activities.length ? observedNumber(subagents) : tr("usage.unavailable")));
  if (usage) {
    if (usage.tokens) panel.append(tokenRows(usage.tokens, tr(usage.tokens.scope === "conversation" ? "usage.conversation" : usage.tokens.scope === "model_call" ? "usage.modelCall" : "usage.turn")));
    if (usage.lastTurnTokens) panel.append(tokenRows(usage.lastTurnTokens, tr(usage.lastTurnTokens.scope === "model_call" ? "usage.modelCall" : "usage.turn")));
    if (usage.context) panel.append(h("div", { class: "usage-block" }, h("strong", { text: tr("usage.context") }), row(tr("usage.used"), `${observedNumber(usage.context.used)} / ${observedNumber(usage.context.capacity)}`), row(tr("usage.source"), usage.context.source)));
    panel.append(h("strong", { text: tr("usage.quota") }));
    if (usage.quota.availability === "unavailable" || !usage.quota.windows.length) panel.append(h("div", { class: "chat-muted", text: usage.quota.message || tr("usage.unavailable") }));
    for (const window of usage.quota.windows) {
      const awaiting = window.state === "awaiting_update" || (window.resetsAt !== null && window.resetsAt <= Date.now() / 1000);
      panel.append(h("div", { class: "usage-block" }, h("strong", { text: rateWindowTitle(window) }), awaiting ? h("div", { class: "chat-muted", text: tr("usage.awaiting") }) : row(tr("usage.used"), observedNumber(window.usedPercent, "%")), awaiting ? null : row(tr("usage.remaining"), observedNumber(window.remainingPercent, "%")), row(tr("usage.duration"), reportedDuration(window.durationMinutes)), row(tr("usage.identifier"), window.id), row(tr("usage.reset"), time(window.resetsAt)), row(tr("usage.source"), window.source), row(tr("usage.captured"), time(window.capturedAt))));
    }
    panel.append(row(tr("usage.version"), usage.sourceVersion || tr("usage.unavailable")), row(tr("usage.captured"), time(usage.capturedAt)));
  } else panel.append(h("div", { class: "chat-muted", text: tr("usage.unavailable") }));
  panel.append(h("p", { class: "chat-muted", text: tr("usage.noGuess") }));
  return h("details", { class: "chat-usage" }, h("summary", { text: `${tr("usage.title")} · ${observedNumber(usage?.tokens?.total)} ${tr("usage.tokens").toLowerCase()}` }), panel);
}
