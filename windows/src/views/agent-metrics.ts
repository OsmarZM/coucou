import { h } from "./dom";
import { countLabel, type AgentMetrics } from "../core/agent-metrics";
import { tr } from "../core/i18n";
import { rateWindowTitle, reportedDuration } from "../core/usage-format";

const number = (value: number | null) => value === null ? "Indisponível" : value.toLocaleString("pt-BR", { maximumFractionDigits: 2 });
const date = (seconds: number | null) => seconds === null ? "Indisponível" : new Date(seconds * 1000).toLocaleString("pt-BR");
const row = (label: string, value: string) => h("div", { class: "usage-row" }, h("span", { text: label }), h("span", { text: value }));

export function agentMetricsBadge(metrics: AgentMetrics): HTMLSpanElement | null {
  if (!metrics.badge && !metrics.awaitingApproval) return null;
  const pending = metrics.awaitingApproval ? ` · ${metrics.awaitingApproval} aguardando ação` : "";
  const description = `${metrics.activeTotal} sessões ocupadas${metrics.externalUnknown ? ` · ${metrics.externalUnknown} externas sem atualização` : ""}${pending}`;
  return h("span", { class: "agent-metrics-badge", text: metrics.badge || "!", title: description, "aria-label": description, style: "font-size:9px;line-height:1;padding:2px 4px;border-radius:5px;background:rgba(255,255,255,.12);margin-left:4px" });
}

/** A compact details panel can be inserted below any existing provider row. */
export function agentMetricsPanel(metrics: AgentMetrics): HTMLDetailsElement {
  const panel = h("div", { class: "usage-panel" },
    row("Sessões ocupadas", countLabel(metrics.sessions)),
    row("Conversas do Coucou", number(metrics.managedActive)),
    row("Sessões externas recentes", number(metrics.externalActive)),
    metrics.externalUnknown ? row("Sessões externas sem atualização", number(metrics.externalUnknown)) : null,
    row("Aguardando autorização ou ação", number(metrics.awaitingApproval)),
    row("Subagentes em execução", countLabel(metrics.subagents)),
    row("Ferramentas em execução", countLabel(metrics.tools)),
    row("Processos locais", countLabel(metrics.processes)),
    h("p", { class: "chat-muted", text: metrics.processes.basis }),
    row("Tokens das conversas observadas", number(metrics.tokens.total)),
    h("p", { class: "chat-muted", text: metrics.tokens.basis }),
  );
  for (const [index, conversation] of metrics.tokens.conversations.entries()) {
    const scope = conversation.scope === "conversation" ? "Conversa" : conversation.scope === "model_call" ? "Última chamada" : conversation.scope === "turn" ? "Último turno" : "Uso indisponível";
    panel.append(row(`${scope} ${index + 1}`, number(conversation.total)), row("Captura", date(conversation.capturedAt)));
  }
  panel.append(h("strong", { text: "Cota da conta" }));
  const quota = metrics.quota;
  if (!quota || quota.availability === "unavailable" || !quota.windows.length) panel.append(h("p", { class: "chat-muted", text: metrics.quotaReason || "A CLI não reportou cota para esta conta." }));
  if (quota) {
    if (quota.historical) panel.append(h("p", { class: "chat-muted", text: "Último snapshot observado; nenhuma conversa deste fornecedor está ativa." }));
    if (quota.quality === "partial") panel.append(h("p", { class: "chat-muted", text: "Cota desta sessão; identidade da conta indisponível." }));
    for (const window of quota.windows) {
      panel.append(h("div", { class: "usage-block" }, h("strong", { text: rateWindowTitle(window) }),
        window.state === "awaiting_update" ? h("p", { class: "chat-muted", text: "Aguardando atualização após a renovação." }) : row("Disponível", `${number(window.remainingPercent)}${window.remainingPercent === null ? "" : "%"}`),
        row(tr("usage.duration"), reportedDuration(window.durationMinutes)),
        row(tr("usage.identifier"), window.id),
        row("Renova em", date(window.resetsAt)), row("Origem", window.source), row("Captura", date(window.capturedAt))));
    }
  }
  panel.append(h("p", { class: "chat-muted", text: metrics.sessions.basis }), h("p", { class: "chat-muted", text: metrics.subagents.basis }));
  return h("details", { class: "agent-metrics-details chat-usage" }, h("summary", { text: "Atividade e uso" }), panel);
}
