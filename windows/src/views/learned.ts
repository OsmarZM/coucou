import { h, clear } from "./dom";
import { Bridge } from "../core/bridge";
import { Personal, type PersonalTab } from "../core/personal";
import { AgentChat, chatDiagnostic } from "../core/agent-chat";
import { State } from "../core/state";
import { tr, type TextKey } from "../core/i18n";
import type { MemoryRecord, SkillRecord, ReviewState, MemoryCandidate } from "../core/personal-types";
import type { ViewActions, ViewHost } from "./views";

function download(content: string, filename: string, type: string) {
  const url = URL.createObjectURL(new Blob([content], { type }));
  const anchor = h("a", { href: url, download: filename });
  anchor.click(); window.setTimeout(() => URL.revokeObjectURL(url), 1000);
}
export function buildLearned(actions: ViewActions): ViewHost {
  const tabs: [PersonalTab, TextKey][] = [["memories", "personal.memories"], ["skills", "personal.skills"], ["history", "personal.history"]];
  const tabButtons = tabs.map(([tab, label]) => h("button", { class: "chat-action", text: tr(label), onclick: () => { Personal.tab = tab; selectedDetail = null; void Personal.refresh(); } }));
  const search = h("input", { class: "chat-config-input", placeholder: tr("personal.search"), "aria-label": tr("personal.search"), maxlength: "1024" }) as HTMLInputElement;
  const filter = h("select", { class: "chat-select", "aria-label": tr("personal.all") }) as HTMLSelectElement;
  filter.append(h("option", { value: "", text: tr("personal.all") }));
  for (const value of ["pending", "approved", "rejected", "superseded"] as const) filter.append(h("option", { value, text: tr(`personal.${value}`) }));
  const refresh = h("button", { class: "chat-action", text: tr("personal.refresh"), onclick: () => { void Personal.refresh(); } });
  const add = h("button", { class: "chat-action", text: "+", "aria-label": tr("personal.memoryNew"), onclick: () => showMemoryEditor(null) });
  const exported = h("button", { class: "chat-action", text: tr("personal.export"), onclick: () => { void run(async () => download(await Bridge.memoryExport(true), "coucou-contexto.json", "application/json;charset=utf-8")); } });
  const controls = h("div", { class: "chat-controls" }, search, filter, refresh, add, exported);
  const notice = h("div", { class: "chat-notice", role: "status" });
  const hint = h("div", { class: "chat-muted" });
  const list = h("div", { class: "learned-list" });
  const detail = h("div", { class: "learned-detail" });
  const el = h("div", { class: "view" }, h("div", { class: "card wash chat-card" }, h("div", { class: "chat-body learned-body" }, h("div", { class: "chat-controls" }, ...tabButtons), controls, notice, hint, list, detail)));
  (el.querySelector(".card") as HTMLElement).style.setProperty("--wash", "rgba(99,102,241,0.5)");
  let renderedVersion = -1;
  let actionBusy = false;
  let selectedDetail: string | null = null;
  let localError: string | null = null;
  let queryTimer: number | null = null;
  let markdownDraft = "";
  search.addEventListener("input", () => {
    Personal.query = search.value;
    if (queryTimer !== null) window.clearTimeout(queryTimer);
    queryTimer = window.setTimeout(() => { queryTimer = null; void Personal.refresh(); }, 250);
  });
  filter.addEventListener("change", () => { Personal.state = filter.value ? filter.value as ReviewState : null; void Personal.refresh(); });
  el.addEventListener("keydown", (event) => event.stopPropagation());
  el.addEventListener("pointerdown", () => { void Bridge.focusWindow(true); });

  async function run(action: () => Promise<unknown>) {
    if (actionBusy) return;
    actionBusy = true; localError = null; State.notify();
    try {
      await Bridge.focusWindow(true);
      if (State.view !== "learned" || State.paused) throw new Error(tr("personal.openToReview"));
      await action();
      await Personal.refresh();
    } catch (error) { localError = chatDiagnostic(error); }
    finally { actionBusy = false; State.notify(); }
  }
  function button(label: TextKey, action: () => Promise<unknown>) { return h("button", { class: "chat-action", text: tr(label), onclick: () => { void run(action); } }); }
  function showText(title: string, text: string) {
    selectedDetail = title; clear(detail);
    detail.append(h("strong", { text: title }), h("pre", { text }), h("button", { class: "chat-action", text: tr("personal.cancel"), onclick: closeDetail }));
    State.notify();
  }
  function closeDetail() { selectedDetail = null; clear(detail); State.notify(); }
  function showMemoryEditor(record: MemoryRecord | null, reusable = false) {
    selectedDetail = record?.id ?? "new-memory"; clear(detail);
    const name = h("input", { class: "chat-config-input", placeholder: tr("personal.name"), maxlength: "200", "aria-label": tr("personal.name") }) as HTMLInputElement;
    const content = h("textarea", { class: "personal-editor", "data-chat-draft": "true", placeholder: tr("personal.content"), maxlength: "8192", "aria-label": tr("personal.content") }) as HTMLTextAreaElement;
    const kind = h("select", { class: "chat-select", "aria-label": tr("personal.content") }) as HTMLSelectElement;
    const scope = h("input", { class: "chat-config-input", "aria-label": tr("personal.scope"), maxlength: "160" });
    scope.value = reusable ? "user" : record?.scope ?? "user";
    scope.readOnly = record !== null;
    for (const [value, label] of [["preference", "personal.preference"], ["fact", "personal.fact"], ["procedure", "personal.procedure"]] as const) kind.append(h("option", { value, text: tr(label) }));
    name.value = record?.key ?? ""; content.value = record?.content ?? ""; kind.value = record?.kind ?? "fact";
    detail.append(name, kind, h("label", { class: "chat-muted" }, tr("personal.scope"), scope), h("p", { class: "chat-muted", hidden: !reusable, text: tr("personal.reuseHint") }), record ? h("pre", { text: `${tr("personal.origin")}: ${record.source.kind} · ${record.source.reference}\n${tr("personal.evidence")}: ${record.source.evidence}` }) : h("span", { hidden: true }), content, h("div", { class: "chat-controls" }, button("personal.propose", async () => {
      const candidate: MemoryCandidate = { id: reusable ? null : record?.id ?? null, expectedRevision: reusable ? null : record?.revision ?? null, kind: kind.value as MemoryCandidate["kind"], key: name.value, content: content.value, scope: scope.value, source: reusable && record ? record.source : { kind: "user", reference: record ? `island:manual-edit:${record.id}` : "island:manual-edit", evidence: tr("personal.manualSource") }, confidence: reusable && record ? record.confidence : 1 };
      await Bridge.memoryPropose(candidate); closeDetail();
    }), h("button", { class: "chat-action", text: tr("personal.cancel"), onclick: closeDetail })));
    State.notify();
  }
  function showSkillEditor(record: SkillRecord, reusable = false) {
    selectedDetail = record.id; clear(detail);
    const body = h("textarea", { class: "personal-editor", "data-chat-draft": "true", "aria-label": tr("personal.content"), maxlength: "32768" }) as HTMLTextAreaElement;
    body.value = record.body;
    detail.append(h("strong", { text: record.name }), h("div", { class: "chat-muted", text: `${tr("personal.scope")}: ${reusable ? "user" : record.scope}` }), h("p", { class: "chat-muted", hidden: !reusable, text: tr("personal.reuseHint") }), h("pre", { text: `${tr("personal.origin")}: ${record.source.kind} · ${record.source.reference}\n${tr("personal.evidence")}: ${record.source.evidence}` }), body, h("div", { class: "chat-controls" }, button("personal.propose", async () => {
      await Bridge.skillsPropose({ id: reusable ? null : record.id, expectedRevision: reusable ? null : record.revision, name: record.name, description: record.description, body: body.value, scope: reusable ? "user" : record.scope, ownership: record.ownership, source: reusable ? record.source : { kind: "user", reference: `island:manual-edit:${record.id}`, evidence: tr("personal.manualSource") } }); closeDetail();
    }), h("button", { class: "chat-action", text: tr("personal.cancel"), onclick: closeDetail })));
    State.notify();
  }
  function recordCard(record: MemoryRecord | SkillRecord, skill: boolean) {
    const title = "name" in record ? record.name : record.key;
    const content = "body" in record ? record.body : record.content;
    const reviewed = h("input", { type: "checkbox" }) as HTMLInputElement;
    const sourceLabel = tr(`personal.${record.source.kind}`);
    const meta = `${tr(`personal.${record.state}`)} · ${tr("personal.revision")} ${record.revision} · ${record.scope}${"version" in record ? ` · ${tr("personal.version")} ${record.version} · ${tr("personal.ownership")}: ${record.ownership}` : ` · ${tr("personal.confidence")}: ${Math.round(record.confidence * 100)}%`}`;
    const card = h("details", { class: "learned-record" }, h("summary", {}, h("strong", { text: title }), h("span", { class: "chat-muted", text: meta })), h("pre", { text: content }), h("div", { class: "chat-muted", text: `${tr("personal.origin")}: ${sourceLabel} · ${record.source.reference}` }), h("pre", { class: "learned-evidence", text: `${tr("personal.evidence")}: ${record.source.evidence}` }));
    const controls = h("div", { class: "chat-controls learned-actions" });
    if (record.state === "pending") {
      const approve = button("personal.approve", () => skill ? Bridge.skillsApprove(record.id, record.revision) : Bridge.memoryApprove(record.id, record.revision));
      approve.disabled = true;
      let diffLoaded = !skill;
      const diffContent = h("pre", { text: skill ? tr("personal.loading") : "" });
      if (skill) {
        card.append(h("strong", { text: tr("personal.diff") }), diffContent);
        card.addEventListener("toggle", () => {
          if (!card.open || diffLoaded) return;
          void Bridge.skillsDiff(record.id, record.revision).then((diff) => {
            diffContent.textContent = diff.diff; diffLoaded = true; approve.disabled = !reviewed.checked || actionBusy;
          }).catch((error) => { diffContent.textContent = chatDiagnostic(error); });
        });
      }
      reviewed.addEventListener("change", () => { approve.disabled = !reviewed.checked || actionBusy || !diffLoaded; });
      card.append(h("label", { class: "chat-write review-check" }, reviewed, h("span", { text: tr("personal.reviewed") })));
      controls.append(approve, button("personal.reject", () => skill ? Bridge.skillsReject(record.id, record.revision) : Bridge.memoryReject(record.id, record.revision)));
    }
    controls.append(h("button", { class: "chat-action", text: tr("personal.edit"), onclick: () => skill ? showSkillEditor(record as SkillRecord) : showMemoryEditor(record as MemoryRecord) }));
    if (record.scope !== "user") controls.append(h("button", { class: "chat-action", text: tr("personal.reuse"), onclick: () => skill ? showSkillEditor(record as SkillRecord, true) : showMemoryEditor(record as MemoryRecord, true) }));
    if (skill) {
      controls.append(button("personal.diff", async () => { const diff = await Bridge.skillsDiff(record.id, record.revision); showText(tr("personal.diff"), diff.diff); }), button("personal.export", async () => { const exported = await Bridge.skillsExport(record.id, record.revision); download(exported.content, exported.filename, "text/markdown;charset=utf-8"); }));
      if (["approved", "superseded"].includes(record.state)) controls.append(button("personal.restore", async () => { await Bridge.skillsRestore(record.id); }));
    }
    controls.append(button("personal.forget", async () => {
      if (!window.confirm(tr("personal.deleteConfirm"))) return;
      if (skill) await Bridge.skillsForget(record.id, record.revision); else await Bridge.memoryForget(record.id, record.revision);
    }));
    card.append(controls); return card;
  }
  function importPanel() {
    const panel = h("div", { class: "personal-preferences" });
    const markdown = h("textarea", { class: "personal-editor", "data-chat-draft": "true", placeholder: tr("personal.markdown"), "aria-label": tr("personal.markdown"), maxlength: "35000" }) as HTMLTextAreaElement;
    markdown.value = markdownDraft;
    markdown.addEventListener("input", () => { markdownDraft = markdown.value; });
    panel.append(h("details", { class: "learned-record" }, h("summary", { text: tr("personal.importSkill") }), markdown, button("personal.propose", async () => { await Bridge.skillsImport(markdown.value, "user"); markdown.value = ""; markdownDraft = ""; Personal.tab = "skills"; })));
    return panel;
  }
  return {
    el,
    sync() {
      if (State.view === "learned" && Personal.dirty && !Personal.busy) void Personal.refresh();
      tabButtons.forEach((button, index) => button.classList.toggle("on", Personal.tab === tabs[index][0]));
      filter.hidden = Personal.tab === "history";
      add.hidden = Personal.tab !== "memories";
      hint.textContent = tr(Personal.tab === "history" ? "personal.historyHint" : "personal.reviewHint");
      notice.textContent = localError || Personal.error || (Personal.busy || actionBusy ? tr("personal.loading") : "");
      notice.hidden = !notice.textContent;
      list.hidden = selectedDetail !== null; detail.hidden = selectedDetail === null;
      if (renderedVersion === Personal.version) return;
      renderedVersion = Personal.version; clear(list);
      if (Personal.tab === "memories") for (const record of Personal.memories) list.append(recordCard(record, false));
      else if (Personal.tab === "skills") { list.append(importPanel()); for (const record of Personal.skills) list.append(recordCard(record, true)); }
      else for (const session of Personal.sessions) {
        const excerpt = Personal.searchHits.find((hit) => hit.session.conversationId === session.conversationId)?.excerpt;
        const record = h("div", { class: "learned-record" }, h("strong", { text: session.title }), h("div", { class: "chat-muted", text: `${session.agent} · ${session.cwd || tr("chat.personal")} · ${session.messageCount} · ${new Date(session.updatedAt).toLocaleString("pt-BR")}` }), excerpt ? h("pre", { text: excerpt }) : null, h("div", { class: "chat-controls" }, button("personal.resume", async () => { if (!window.confirm(`${tr("personal.resumeConfirm")}\n${session.title}\n${session.sessionId ?? session.conversationId}`)) return; const navigation = State.navigationGeneration; const conversation = await Personal.resume(session); if (conversation && navigation === State.navigationGeneration && AgentChat.current?.id === conversation.id && State.view === "learned" && !State.paused) actions.setView("prompt"); }), button("personal.forget", async () => {
          if (!window.confirm(tr("personal.historyConfirm"))) return;
          if (!await Personal.forgetConversation(session.conversationId)) localError = tr("personal.historyStopped");
        })));
        list.append(record);
      }
      if (!list.childElementCount && !Personal.busy) list.append(h("div", { class: "chat-empty", text: tr("personal.empty") }));
      if (actionBusy) for (const button of list.querySelectorAll("button")) button.disabled = true;
    },
  };
}
