import { h, svg, clear } from "./dom";
import { ICONS } from "./icons";
import { Bridge, IS_TAURI, type ChatContext } from "../core/bridge";
import { AgentChat, PERSONAL_CONTEXT_ID, cancelAgentChat, chatDiagnostic, decideAgentChat, denyAgentChatApproval, refreshAgentChatStatus, sendAgentChat, setApprovalInteractionGuard, type AgentConversation } from "../core/agent-chat";
import { Attachments, attachmentScopeBusy, ingestConversationAttachments, setAttachmentConversationResolver } from "../core/attachments";
import { Personal } from "../core/personal";
import { tr, formatText } from "../core/i18n";
import { State } from "../core/state";
import { Sound } from "../core/sound";
import { usagePanel } from "./usage";
import { AGENT_META, type AgentId } from "../core/sessions";
import { createMiniBot } from "../mochi/minibots";
import type { ViewHost } from "./views";

const PROVIDERS = [["codex", "Codex CLI"], ["claude", "Claude Code"], ["gemini", "Gemini CLI"], ["copilot", "GitHub Copilot CLI"], ["anthropic", "Anthropic API"]];
const agentLabel = (agent: string) => PROVIDERS.find(([id]) => id === agent)?.[1] ?? agent;
const projectName = (cwd: string) => cwd.replace(/[\\/]+$/, "").split(/[\\/]/).pop() || tr("chat.personal");
function bubble(role: "user" | "assistant", content: string, agent?: string, createdAt?: number, truncated = false) {
  const row = h("div", { class: role === "user" ? "chat-row user" : "chat-row" });
  const text = h("div", { class: role === "user" ? "bubble" : "reply" });
  if (agent) text.append(h("div", { class: "chat-message-meta", text: `${role === "user" ? tr("chat.you") : agentLabel(agent)}${createdAt ? ` · ${new Date(createdAt).toLocaleTimeString("pt-BR", { hour: "2-digit", minute: "2-digit" })}` : ""}`, title: createdAt ? new Date(createdAt).toLocaleString("pt-BR") : "" }));
  text.append(h("span", { text: content })); if (truncated) text.append(h("div", { class: "chat-message-meta", text: tr("chat.historyExcerpt") })); row.append(text); return row;
}

export function buildPrompt(onHeightChange: () => void, openLearned: () => void = () => {}): ViewHost {
  const characters = h("div", { class: "chat-characters", role: "group", "aria-label": tr("chat.characters") });
  const characterButtons = new Map<AgentId, HTMLButtonElement>();
  for (const agent of ["codex", "claude", "gemini", "copilot"] as const) {
    const meta = AGENT_META[agent]; const task = State.tasks.find((entry) => entry.id === meta.taskId);
    const button = h("button", { class: "chat-character", "aria-label": formatText("chat.chooseCharacter", { agent: meta.name }), title: meta.name }, task ? createMiniBot(task, 18) : h("span", { text: meta.name[0] }), h("span", { text: meta.name.replace(" CLI", "").replace("GitHub ", "") }));
    button.style.setProperty("--character-color", meta.color); characters.append(button); characterButtons.set(agent, button);
    button.addEventListener("click", () => {
      saveDraft(); const current = AgentChat.current;
      if (current?.approval) void denyAgentChatApproval(current.id);
      AgentChat.selectPersonal(agent); State.setFocus(meta.taskId); selectedForm = ""; formDirty = false; localError = null; State.notify();
    });
  }
  const conversations = h("select", { class: "chat-select chat-conversations", "aria-label": tr("chat.conversation") });
  const newButton = h("button", { class: "chat-action", text: tr("chat.applyProject"), title: tr("chat.newHint") });
  const refresh = h("button", { class: "chat-action", text: "↻", title: tr("chat.refresh"), "aria-label": tr("chat.refresh") });
  const learned = h("button", { class: "chat-action", text: tr("personal.title"), onclick: openLearned });
  const cwd = h("input", { class: "chat-config-input chat-project", placeholder: tr("chat.folderPlaceholder"), "aria-label": tr("chat.folder"), spellcheck: "false" });
  const writable = h("input", { type: "checkbox", "aria-label": tr("chat.write") });
  const writeLabel = h("label", { class: "chat-write" }, writable, h("span", { text: tr("chat.write") }));
  const resume = h("input", { class: "chat-config-input", placeholder: tr("chat.resume"), "aria-label": tr("chat.resume"), spellcheck: "false", autocomplete: "off" });
  const copySession = h("button", { class: "chat-action", text: tr("chat.copy"), "aria-label": tr("chat.copyHint") });
  const revoke = h("button", { class: "chat-action", text: tr("chat.revoke") });
  const api = h("button", { class: "chat-action", text: tr("chat.apiManual") });
  const advanced = h("details", { class: "chat-advanced" }, h("summary", { text: tr("chat.advanced") }), h("div", { class: "chat-controls" }, cwd, writeLabel), h("div", { class: "chat-controls" }, resume, copySession, newButton), h("div", { class: "chat-controls" }, conversations, refresh, api), h("div", { class: "chat-controls" }, h("span", { class: "chat-muted", text: tr("chat.closeTerminal") }), revoke));
  const notice = h("div", { class: "chat-notice", role: "status" });
  const chipRow = h("div", { class: "chat-attachments" });
  const attachmentScope = h("div", { class: "chat-muted", text: tr("chat.sharedAttachments") });
  const log = h("div", { class: "chat-log", role: "log", "aria-label": tr("chat.messages") });
  const approvalBox = h("div", { class: "chat-permission" });
  const input = h("input", { type: "text", class: "chat-input", "aria-label": tr("chat.message"), maxlength: "16384" });
  const send = h("button", { class: "send-btn", title: tr("chat.send"), "aria-label": tr("chat.send") }, svg(ICONS.arrowUp, 11));
  const stop = h("button", { class: "chat-action chat-stop", text: tr("chat.stop") });
  const attach = h("button", { class: "chat-action", title: tr("chat.attachHint"), "aria-label": tr("chat.attach") }, svg(ICONS.plus, 11));
  const footer = h("div", { class: "chat-muted chat-storage" });
  const el = h("div", { class: "view" }, h("div", { class: "card wash chat-card" }, h("div", { class: "chat-body" }, h("div", { class: "chat-controls" }, characters, learned), advanced, notice, attachmentScope, chipRow, log, approvalBox, h("div", { class: "chat-bar" }, attach, input, stop, send), footer)));
  (el.querySelector(".card") as HTMLElement).style.setProperty("--wash", "rgba(99,102,241,0.5)");
  let apiSending = false;
  let apiNextId = Math.max(0, ...State.chatHistory.map((message) => message.id)) + 1;
  let selectedForm = "", draftKey = "", renderedKey = "", permissionKey = "";
  let formDirty = false, resumeEdited = false;
  let localError: string | null = null, nativeSessionShown: string | null = null;
  const drafts = new Map<string, string>();
  AgentChat.selectPersonal(AgentChat.provider === "anthropic" ? "codex" : AgentChat.provider);
  setApprovalInteractionGuard(() => State.mode === "expanded" && State.view === "prompt" && !State.paused);
  if (IS_TAURI && !Personal.preferencesLoaded) void Personal.loadPreferences();
  function saveDraft() {
    if (draftKey === PERSONAL_CONTEXT_ID) AgentChat.setPersonalDraft(input.value);
    if (draftKey) drafts.set(draftKey, input.value);
    if (AgentChat.current?.id === draftKey) AgentChat.current.draft = input.value;
    while (drafts.size > 32) drafts.delete(drafts.keys().next().value!);
  }
  input.addEventListener("input", saveDraft);
  function fillForm(conversation: AgentConversation | null) {
    cwd.value = conversation?.cwd ?? ""; writable.checked = conversation?.writable ?? false;
    resume.value = conversation?.sessionId ?? ""; nativeSessionShown = conversation?.sessionId ?? null;
    formDirty = false; resumeEdited = false;
  }
  function createConversation(carryDraft = true) {
    if (AgentChat.provider === "anthropic") throw new Error(tr("chat.apiAttachments"));
    saveDraft(); const draft = carryDraft ? input.value : "";
    const conversation = AgentChat.create({ agent: AgentChat.provider, cwd: cwd.value, writable: !!cwd.value.trim() && writable.checked, sessionId: resumeEdited ? resume.value : null, contextId: null });
    conversation.draft = draft; drafts.set(conversation.id, draft);
    selectedForm = conversation.id; fillForm(conversation); localError = null; onHeightChange(); return conversation;
  }
  function formConversation() { if (formDirty) throw new Error(tr("chat.applyBeforeSend")); const current = AgentChat.current; return !current || current.agent !== AgentChat.provider ? AgentChat.selectPersonal(AgentChat.provider as AgentId) : current; }
  setAttachmentConversationResolver(formConversation);
  function changeConfig() { saveDraft(); if (!resumeEdited) resume.value = ""; formDirty = true; localError = null; State.notify(); }
  cwd.addEventListener("input", changeConfig); writable.addEventListener("change", changeConfig);
  resume.addEventListener("input", () => { resumeEdited = true; changeConfig(); });
  copySession.addEventListener("click", async () => {
    if (!resume.value) return;
    try { await navigator.clipboard.writeText(resume.value); localError = tr("chat.copied"); }
    catch { localError = tr("chat.copyManual"); resume.focus(); resume.select(); } State.notify();
  });
  revoke.addEventListener("click", async () => {
    const conversation = AgentChat.current; if (!conversation) return;
    try { await Bridge.focusWindow(true); await denyAgentChatApproval(conversation.id); await Bridge.permissionsRevoke(conversation.id); localError = tr("chat.revoked"); }
    catch (error) { localError = chatDiagnostic(error); } State.notify();
  });
  api.addEventListener("click", () => { saveDraft(); if (AgentChat.current?.approval) void denyAgentChatApproval(AgentChat.current.id); AgentChat.setProvider("anthropic"); selectedForm = ""; localError = null; State.notify(); });
  conversations.addEventListener("change", () => {
    const chosen = AgentChat.conversations.find((entry) => entry.id === conversations.value);
    if (!chosen || !window.confirm(formatText("chat.openChannelConfirm", { agent: agentLabel(chosen.agent), session: chosen.sessionId ?? chosen.id, cwd: chosen.cwd || tr("chat.personal") }))) { conversations.value = AgentChat.current?.id ?? ""; return; }
    saveDraft(); const current = AgentChat.current;
    if (current?.approval && current.id !== conversations.value) void denyAgentChatApproval(current.id);
    if (AgentChat.select(conversations.value)) { selectedForm = ""; localError = null; State.notify(); }
  });
  newButton.addEventListener("click", () => {
    try { if (!formDirty) return; if (!window.confirm(formatText("chat.openChannelConfirm", { agent: agentLabel(AgentChat.provider), session: resumeEdited && resume.value ? resume.value : tr("chat.newSession"), cwd: cwd.value || tr("chat.personal") }))) return; if (AgentChat.current?.approval) void denyAgentChatApproval(AgentChat.current.id); if (!resumeEdited) resume.value = ""; createConversation(); }
    catch (error) { localError = chatDiagnostic(error); State.notify(); }
  });
  refresh.addEventListener("click", () => { void refreshAgentChatStatus(); if (AgentChat.current) void Attachments.refresh(AgentChat.current.id); });
  stop.addEventListener("click", () => { const conversation = AgentChat.current; if (conversation?.runId) void cancelAgentChat(conversation.id, conversation.runId); });
  attach.addEventListener("click", async () => {
    try { const conversation = formConversation(); const paths = await Bridge.documentsChoose(); if (paths.length) await ingestConversationAttachments(conversation.id, paths); }
    catch (error) { localError = chatDiagnostic(error); State.notify(); }
  });
  async function submit() {
    const query = input.value.trim(); if (!query || State.paused) return; localError = null;
    if (AgentChat.provider !== "anthropic") {
      try {
        const availability = AgentChat.statuses.find((status) => status.agent === AgentChat.provider);
        if (availability && !availability.available) throw new Error(availability.detail || tr("chat.unavailable"));
        if (availability?.readonlyOnly && writable.checked) throw new Error(tr("chat.readonly"));
        if (!cwd.value.trim() && !(availability?.personalSupported ?? ["codex", "claude"].includes(AgentChat.provider))) throw new Error(tr("chat.personalUnavailable"));
        const conversation = formConversation(); if (conversation.runId || Attachments.get(conversation.id).busy) return;
        const submittedDraft = input.value;
        const accepted = await sendAgentChat(conversation, query, Attachments.selectedIds(conversation.id));
        if (accepted) {
          conversation.draft = ""; drafts.set(conversation.id, "");
          if (conversation.contextId === PERSONAL_CONTEXT_ID && AgentChat.personalDraft === submittedDraft) { AgentChat.setPersonalDraft(""); drafts.set(PERSONAL_CONTEXT_ID, ""); }
          if ((draftKey === conversation.id || draftKey === conversation.contextId) && input.value === submittedDraft) input.value = "";
          Sound.play("send");
        }
      } catch (error) { localError = chatDiagnostic(error); }
      State.notify(); onHeightChange(); return;
    }
    if (apiSending) return;
    apiSending = true; input.value = ""; drafts.set("api", ""); Sound.play("send");
    State.chatHistory.push({ id: apiNextId++, role: "user", content: query }); State.stateOverride = "thinking"; State.notify(); onHeightChange();
    const pending = State.promptContext;
    const context: ChatContext | null = pending?.kind === "window" ? pending : pending?.path ? { kind: "file", name: pending.name, path: pending.path } : null;
    State.promptContext = null;
    try { const reply = await Bridge.chatSend(query, context); State.chatHistory.push({ id: apiNextId++, role: "assistant", content: reply.text }); Sound.play("finish"); }
    catch (error) { localError = chatDiagnostic(error); Sound.play("error"); }
    finally { apiSending = false; State.stateOverride = null; State.notify(); onHeightChange(); }
  }
  send.addEventListener("click", () => { void submit(); });
  input.addEventListener("keydown", (event) => { if (event.key === "Enter") { event.preventDefault(); void submit(); } event.stopPropagation(); });
  for (const field of [input, cwd, resume, conversations]) { field.addEventListener("keydown", (event) => event.stopPropagation()); field.addEventListener("pointerdown", () => { void Bridge.focusWindow(true); }); }
  let resetClockKey = "";
  return { el, tick() {
    const quota = AgentChat.current?.usage?.quota;
    const expired = quota?.windows.filter((window) => window.resetsAt !== null && window.resetsAt <= Date.now() / 1000).map((window) => `${window.id}:${window.resetsAt}`).join("|") ?? "";
    if (expired !== resetClockKey) { resetClockKey = expired; renderedKey = ""; State.notify(); }
  }, sync() {
    const cli = AgentChat.provider !== "anthropic";
    const conversation = cli && AgentChat.current?.agent === AgentChat.provider ? AgentChat.current : null;
    for (const [agent, button] of characterButtons) { button.setAttribute("aria-pressed", String(AgentChat.provider === agent)); button.classList.toggle("on", AgentChat.provider === agent); button.disabled = apiSending; }
    conversations.disabled = apiSending || !cli; newButton.disabled = apiSending || !cli; newButton.hidden = !cli; refresh.hidden = !cli;
    advanced.hidden = !!conversation?.approval; chipRow.hidden = !!conversation?.approval; footer.hidden = !!conversation?.approval;
    footer.textContent = tr(cli ? conversation?.contextId === PERSONAL_CONTEXT_ID ? "chat.continuousContext" : "chat.separateChannel" : "chat.apiStorage");
    if (conversation && selectedForm !== conversation.id) { selectedForm = conversation.id; fillForm(conversation); }
    if (conversation && !formDirty && nativeSessionShown !== conversation.sessionId) { resume.value = conversation.sessionId ?? ""; nativeSessionShown = conversation.sessionId; }
    const nextDraftKey = !cli ? "api" : conversation?.contextId === PERSONAL_CONTEXT_ID ? PERSONAL_CONTEXT_ID : conversation?.id ?? `new:${AgentChat.provider}`;
    if (draftKey !== nextDraftKey) { saveDraft(); draftKey = nextDraftKey; input.value = draftKey === PERSONAL_CONTEXT_ID ? AgentChat.personalDraft : drafts.get(draftKey) ?? (conversation?.id === draftKey ? conversation.draft : ""); }
    const availability = AgentChat.statuses.find((status) => status.agent === AgentChat.provider);
    writable.disabled = availability?.readonlyOnly === true || !cwd.value.trim(); copySession.disabled = !resume.value; revoke.disabled = !conversation;
    if (availability?.readonlyOnly && !conversation?.writable) writable.checked = false;
    writeLabel.title = tr(availability?.readonlyOnly ? "chat.readonly" : "chat.writeHint");
    const optionsKey = AgentChat.conversations.map((entry) => `${entry.id}:${entry.agent}:${entry.cwd}:${entry.sessionId}:${entry.status}:${!!entry.approval}`).join("|");
    if (conversations.dataset.options !== optionsKey) {
      conversations.dataset.options = optionsKey; clear(conversations); conversations.append(h("option", { value: "", text: tr("chat.choose") }));
      for (const entry of AgentChat.conversations) conversations.append(h("option", { value: entry.id, text: `${entry.approval ? `${tr("chat.permission")} · ` : ""}${agentLabel(entry.agent)} · ${projectName(entry.cwd)} · ${(entry.sessionId ?? entry.id).slice(-6)} · ${tr(`chat.status.${entry.status}`)}` }));
    }
    conversations.value = cli ? conversation?.id ?? "" : "";
    const active = cli ? !!conversation?.runId : apiSending; const docs = conversation ? Attachments.get(conversation.id) : null;
    const documentsLocked = !!conversation && attachmentScopeBusy(conversation.id);
    attachmentScope.hidden = !docs?.items.length || !!conversation?.approval;
    attachmentScope.textContent = tr(conversation?.contextId === PERSONAL_CONTEXT_ID ? "chat.sharedAttachments" : "chat.channelAttachments");
    if (conversation && IS_TAURI) Attachments.ensureLoaded(conversation.id);
    stop.hidden = !cli || !active; stop.disabled = conversation?.status === "stopping"; stop.textContent = tr(conversation?.status === "stopping" ? "chat.stopping" : "chat.stop");
    attach.hidden = !cli; attach.disabled = documentsLocked || State.paused || !!docs?.busy;
    input.disabled = active || State.paused; send.disabled = active || State.paused || !!docs?.busy || (cli && availability?.available === false);
    input.placeholder = formatText("chat.ask", { agent: cli ? agentLabel(AgentChat.provider) : "Anthropic" });
    const statusText = localError || (State.paused ? tr("chat.paused") : conversation?.error) || docs?.error || (cli ? AgentChat.statusError : null) || (docs?.busy ? tr("chat.attachBusy") : "") || (cli && availability && !availability.available ? availability.detail : "") || (formDirty && cli ? tr("chat.changed") : "") || (conversation?.status === "stopping" ? tr("chat.waitStop") : "") || (cli && !conversation?.messages.length && conversation?.sessionId ? tr("chat.nativeHistory") : "") || (cli && availability?.detail ? availability.detail : "") || (cli && !IS_TAURI ? tr("chat.browser") : "");
    notice.textContent = statusText; notice.hidden = !statusText; notice.title = statusText;
    const attachmentsKey = `${conversation?.id}:${documentsLocked}:${docs?.busy}:${JSON.stringify(docs?.items)}:${JSON.stringify([...(docs?.selected ?? [])])}`;
    if (chipRow.dataset.key !== attachmentsKey) {
      chipRow.dataset.key = attachmentsKey; clear(chipRow);
      if (cli) for (const item of docs?.items ?? []) {
        const selected = h("input", { type: "checkbox", "aria-label": item.name }); selected.checked = docs!.selected.has(item.id); selected.disabled = documentsLocked || !!docs?.busy || !["ready", "queued"].includes(item.status);
        const conversationId = conversation!.id;
        selected.addEventListener("change", () => { if (!attachmentScopeBusy(conversationId)) Attachments.toggle(conversationId, item.id, selected.checked); else State.notify(); });
        const remove = h("button", { class: "attachment-remove", text: "×", "aria-label": `${tr("chat.attachRemove")}: ${item.name}` }); remove.disabled = documentsLocked || !!docs?.busy;
        remove.addEventListener("click", () => { if (!attachmentScopeBusy(conversationId)) void Attachments.remove(conversationId, item.id).catch(() => {}); });
        const coverage = item.coverage;
        const title = [item.name, item.kind.toUpperCase(), `${(item.size / 1024).toLocaleString("pt-BR", { maximumFractionDigits: 1 })} KiB`, tr(item.status === "ready" ? "chat.ready" : item.status === "queued" ? "chat.queued" : item.status === "unsupported" ? "chat.unsupported" : "chat.attachError"), item.message, coverage?.totalPages != null ? formatText("chat.pageCoverage", { read: coverage.readPages ?? 0, total: coverage.totalPages }) : "", coverage?.truncated ? tr("chat.partial") : "", ...(coverage?.notes ?? [])].filter(Boolean).join(" · ");
        chipRow.append(h("span", { class: "attachment-chip", title }, selected, h("span", { text: item.name }), coverage?.truncated ? h("span", { class: "chat-muted", text: tr("chat.partial") }) : null, remove));
      } else if (State.droppedFile) chipRow.append(h("span", { class: "attachment-chip", text: State.droppedFile.name }));
    }
    const messages = cli ? conversation?.contextId === PERSONAL_CONTEXT_ID ? AgentChat.timeline() : conversation?.messages ?? [] : State.chatHistory;
    const logKey = JSON.stringify([AgentChat.provider, conversation?.id, active, messages, AgentChat.contextTruncated, conversation?.tools, conversation?.usage, conversation?.activities, conversation?.processCount, conversation?.context, docs?.preparation]);
    if (renderedKey !== logKey) {
      renderedKey = logKey; const nearEnd = log.scrollHeight - log.scrollTop - log.clientHeight < 30;
      const usageOpen = (log.querySelector(".chat-usage") as HTMLDetailsElement | null)?.open ?? false; clear(log);
      for (const message of messages) if (message.content) log.append(bubble(message.role, message.content, "agent" in message ? message.agent : undefined, "createdAt" in message ? message.createdAt : undefined, "contentTruncated" in message && message.contentTruncated));
      if (conversation?.tools.length) log.append(h("details", { class: "chat-tools" }, h("summary", { text: conversation.tools.at(-1) }), h("pre", { text: conversation.tools.join("\n\n") })));
      if (conversation?.context) log.append(h("div", { class: "chat-muted", text: `${formatText("chat.contextApplied", { memories: conversation.context.memories.length, skills: conversation.context.skills.length })}${conversation.context.truncated ? ` · ${tr("chat.partial")}` : ""}` }));
      if (conversation?.contextId === PERSONAL_CONTEXT_ID && AgentChat.contextTruncated) log.append(h("div", { class: "chat-muted", text: tr("chat.timelineLimited") }));
      if (docs?.preparation) log.append(h("div", { class: "chat-muted", text: `${formatText("chat.documentContext", { used: docs.preparation.usedChars, budget: docs.preparation.budgetChars })}${docs.preparation.partial ? ` · ${tr("chat.partial")}` : ""}` }));
      if (conversation && (conversation.usage || conversation.processCount !== null || Object.keys(conversation.activities).length)) { const usage = usagePanel(conversation); usage.open = usageOpen; log.append(usage); }
      if (active && !conversation?.approval) log.append(h("div", { class: "chat-row", "aria-label": tr("chat.working") }, h("div", { class: "typing" }, h("i"), h("i"), h("i"))));
      if (!messages.length && !active) log.append(h("div", { class: "chat-empty", text: tr(cli ? "chat.empty" : "chat.apiEmpty") }));
      if (nearEnd || messages.length <= 2) log.scrollTop = log.scrollHeight;
    }
    const approval = conversation?.approval;
    const key = approval && conversation?.runId ? `${conversation.id}:${conversation.runId}:${approval.requestId}:${approval.submitting}` : "";
    if (permissionKey !== key) {
      permissionKey = key; clear(approvalBox);
      if (approval && conversation?.runId) {
        const { id: conversationId, runId } = conversation; const { requestId } = approval;
        const buttons = approval.choices.map((decision) => {
          const button = h("button", { class: decision === "deny" ? "chat-action" : "chat-action chat-allow", text: tr(decision === "deny" ? "chat.deny" : decision === "allowConversation" ? "chat.allowConversation" : "chat.allowOnce") }); button.disabled = approval.submitting;
          button.addEventListener("click", () => { void decideAgentChat(conversationId, runId, requestId, decision); }); return button;
        });
        approvalBox.append(h("strong", { text: approval.title || tr("chat.permissionRequired") }), h("pre", { class: "chat-permission-detail", text: approval.detail }), h("div", { class: "chat-controls" }, ...buttons));
      }
    }
    approvalBox.hidden = !approval;
  }, focus() { if (!input.disabled) input.focus(); } };
}
