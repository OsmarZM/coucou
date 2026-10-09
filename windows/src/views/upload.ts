import { tr } from "../core/i18n";
// Drop zone, upload progress and the "what do you want to do with it" card —
// ports of UploadView / UploadingView / ChooseView from IslandViewContent.swift.
//
// Sending a file by email is not in the Windows v1, so `choose` offers the one
// action the spec asks for: ask a question about it.

import { h, clear } from "./dom";
import { State } from "../core/state";
import { AgentChat, PERSONAL_CONTEXT_ID, chatDiagnostic } from "../core/agent-chat";
import { Attachments, attachmentStatusLabel } from "../core/attachments";
import type { ViewActions, ViewHost } from "./views";

/** Dashed rounded rect drawn as SVG so the dashes can march like on macOS. */
function dashedFrame(): SVGSVGElement {
  const ns = "http://www.w3.org/2000/svg";
  const el = document.createElementNS(ns, "svg");
  el.setAttribute("class", "drop-frame");
  el.setAttribute("preserveAspectRatio", "none");
  const rect = document.createElementNS(ns, "rect");
  rect.setAttribute("x", "0.75");
  rect.setAttribute("y", "0.75");
  rect.setAttribute("width", "calc(100% - 1.5px)");
  rect.setAttribute("height", "calc(100% - 1.5px)");
  rect.setAttribute("rx", "20");
  rect.setAttribute("fill", "none");
  rect.setAttribute("stroke-width", "1.5");
  rect.setAttribute("stroke-dasharray", "6 5");
  el.append(rect);
  return el;
}

export function buildUpload(actions: ViewActions): ViewHost {
  const frame = dashedFrame();
  const title = h("div", { class: "drop-title", text: tr("Drop your files here") });
  const tags = h(
    "div",
    { class: "drop-tags" },
    ...["PDF", "DOCX", "TXT", tr("Code")].map((t) => h("span", { text: t })),
  );
  const choose = h("button", { class: "chat-action", text: "Escolher arquivos", onclick: () => actions.chooseFiles() });
  const card = h(
    "div",
    { class: "card drop-card" },
    frame,
    h("div", { class: "drop-body" }, title, tags, choose, h("div", { class: "chat-muted", text: "Até 10 anexos · 20 MiB por arquivo · 50 MiB por lote. Anexar não envia ao modelo." })),
  );
  const el = h("div", { class: "view" }, card);

  return {
    el,
    sync() {
      card.classList.toggle("over", State.fileDragOver);
      choose.disabled = State.paused || State.attachmentBusy;
    },
  };
}

export function buildUploading(actions: ViewActions): ViewHost {
  const label = h("span", { class: "up-name" });
  const percent = h("span", { class: "up-pct" });
  const fill = h("div", { class: "up-fill" });
  const cancel = h("button", { class: "chat-action", text: "Cancelar importação", onclick: async () => {
    const operation = Attachments.lastImport; if (!operation) return;
    try { await Attachments.cancelImport(operation.conversationId); }
    catch (error) { Attachments.reportError(operation.conversationId, chatDiagnostic(error)); }
  } });
  const open = h("button", { class: "chat-action", text: "Abrir conversa", onclick: () => openCapturedConversation(actions) });
  const detail = h("div", { class: "chat-muted upload-status", role: "status" });
  const card = h(
    "div",
    { class: "card up-card" },
    h("div", { class: "up-row" }, label, percent),
    h("div", { class: "up-track", role: "progressbar", "aria-label": "Importação de documentos" }, fill),
    detail, h("div", { class: "up-actions" }, cancel, open),
  );
  const el = h("div", { class: "view" }, card);

  return {
    el,
    sync() {
      const operation = Attachments.lastImport;
      const docs = operation ? Attachments.get(operation.conversationId) : null;
      const busy = !!docs?.busy; const cancelling = operation?.status === "cancelling";
      label.textContent = operation?.names.join(", ") ?? "Documentos";
      percent.textContent = busy ? cancelling ? "Cancelando…" : "Importando…" : operation?.status === "completed" ? "Anexado localmente" : operation?.status === "cancelled" ? "Cancelado" : "Falha";
      label.classList.toggle("done", operation?.status === "completed");
      card.classList.toggle("importing", busy); card.classList.toggle("done", operation?.status === "completed");
      fill.style.width = busy ? "35%" : operation?.status === "completed" ? "100%" : "0";
      detail.textContent = docs?.error || operation?.error || (busy ? "A cópia está em andamento. O conteúdo ainda não foi enviado ao modelo." : "Confira os anexos na conversa antes de enviar.");
      detail.classList.toggle("error", !!docs?.error || !!operation?.error);
      cancel.hidden = !busy; cancel.disabled = cancelling || operation?.status !== "importing";
      cancel.textContent = cancelling ? "Cancelando importação…" : "Cancelar importação";
    },
  };
}

function openCapturedConversation(actions: ViewActions) {
  const contextId = Attachments.lastImport?.conversationId;
  if (contextId && contextId !== PERSONAL_CONTEXT_ID) AgentChat.select(contextId);
  actions.setView("prompt");
}

export function buildChoose(actions: ViewActions): ViewHost {
  const title = h("div", { class: "title" });
  const sub = h("div", { class: "sub" });
  const summarize = h("button", { class: "btn primary", text: "Ler e resumir", onclick: () => {
    const contextId = Attachments.lastImport?.conversationId; if (!contextId) return;
    Attachments.requestSummary(contextId);
    openCapturedConversation(actions);
  } });
  summarize.title = "Abre a conversa para resumir os anexos. O envio ao modelo acontece ao clicar em Enviar ou Ler e resumir no chat.";
  const row = h(
    "div",
    { class: "actions" },
    summarize,
    h("button", {
      class: "btn primary",
      text: tr("Ask a question"),
      onclick: () => openCapturedConversation(actions),
    }),
    h("button", {
      class: "btn secondary",
      text: tr("Cancel"),
      onclick: () => actions.setView(State.defaultView()),
    }),
  );
  const el = h(
    "div",
    { class: "view" },
    h(
      "div",
      { class: "card" },
      h("div", { class: "stack", style: "padding:0 18px 0 98px" }, title, sub, row),
    ),
  );

  return {
    el,
    sync() {
      const operation = Attachments.lastImport; const docs = operation ? Attachments.get(operation.conversationId) : null;
      const imported = docs?.items.filter((item) => operation?.attachmentIds.includes(item.id)) ?? [];
      clear(title); title.append(h("b", { text: operation?.names.join(", ") || "Documentos" }), document.createTextNode(" · anexado localmente"));
      sub.textContent = imported.length ? imported.map((item) => `${item.name}: ${attachmentStatusLabel(item)}`).join(" · ") : "Os arquivos não foram enviados ao modelo.";
      summarize.disabled = !operation || !Attachments.selectedIds(operation.conversationId).length || State.paused || !!docs?.busy;
    },
  };
}
