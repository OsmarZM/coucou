import { tr, formatText, stateLabel } from "../core/i18n";
// Island views — DOM ports of IslandViewContent.swift. Paddings, font sizes,
// colours and wording are copied from the Swift views so both platforms read
// identically.

import { h, svg, clear, dot } from "./dom";
import { ICONS } from "./icons";
import { Ticker } from "./ticker";
import { State, type AgentTask } from "../core/state";
import { AGENT_META, agentForTask } from "../core/sessions";
import { washRGBA, type IslandViewName, type Wash } from "../core/layout";
import { createMiniBot, pruneMiniBots } from "../mochi/minibots";
import { buildPrompt } from "./chat";
import { buildLearned } from "./learned";
import { AgentChat, denyAgentChatApproval } from "../core/agent-chat";
import { aggregateAgentMetrics, type AgentMetrics } from "../core/agent-metrics";
import { agentMetricsBadge, agentMetricsPanel } from "./agent-metrics";
import { AGENT_IDS, type AgentId } from "../core/sessions";
import { buildChoose, buildUpload, buildUploading } from "./upload";
import { renderIntegrationCard, type IntegrationCardHooks } from "./integrations";

export interface ViewActions {
  setView(v: IslandViewName): void;
  collapse(): void;
  togglePin(): void;
  setFocus(id: string): void;
  openTerminal(): void;
  /** The ↗ button: opens whatever the focused pill points at. */
  openTarget(): void;
  openUrl(url: string): void;
  decide(d: "allow" | "deny"): void;
  toggleSound(): void;
  setVolume(v: number): void;
  setAutoClose(seconds: number): void;
  openSettingsWindow(): void;
  blip(): void;
}

export interface ViewHost {
  el: HTMLElement;
  sync(): void;
  /** Called when the view becomes active, for views with a text field. */
  focus?(): void;
  /** Called every frame while the view is on screen. */
  tick?(nowMs: number): void;
}

// ── Shared pieces ─────────────────────────────────────────────────────────────

function card(wash: Wash, ...children: (Node | string)[]): HTMLElement {
  const el = h("div", { class: wash ? "card wash" : "card" }, ...children);
  if (wash) el.style.setProperty("--wash", washRGBA(wash));
  return el;
}

function btn(
  label: string,
  kind: "primary" | "secondary",
  onClick: () => void,
  kbd?: string,
): HTMLElement {
  return h(
    "button",
    { class: `btn ${kind}`, onclick: onClick },
    h("span", { text: label }),
    kbd ? h("span", { class: "kbd", text: kbd }) : null,
  );
}

/** AgentWho — coloured dot + task name + grey label. */
function agentWho(task: AgentTask | null, label: string): HTMLElement {
  const row = h("div", { class: "who-row" });
  if (task) {
    row.append(dot(task.color, 8), h("span", { class: "n", text: task.name }));
  }
  row.append(h("span", { text: label }));
  return row;
}

function agentName(task: AgentTask | null): string {
  const agent = task ? agentForTask(task.id) : null;
  return agent ? AGENT_META[agent].name : task?.name || tr("Agent");
}

function stack(padLeft: number, padRight: number, ...children: Node[]): HTMLElement {
  const el = h("div", { class: "stack" }, ...children);
  el.style.padding = `4px ${padRight}px 4px ${padLeft}px`;
  return el;
}

// ── Header ────────────────────────────────────────────────────────────────────

export function buildHeader(actions: ViewActions): ViewHost {
  const tabHome = h("button", { class: "tab", title: tr("Overview"), onclick: () => go("overview") }, svg(ICONS.house, 13));
  const tabChat = h("button", { class: "tab", title: tr("Ask"), onclick: () => go("prompt") }, svg(ICONS.bubble, 13));
  const tabDrop = h("button", { class: "tab", title: tr("Drop"), onclick: () => go("upload") }, svg(ICONS.plus, 13));
  const tabLearned = h("button", { class: "tab", title: tr("personal.title"), "aria-label": tr("personal.title"), onclick: () => go("learned") }, svg("M4 4h7a3 3 0 0 1 3 3v14a3 3 0 0 0-3-3H4V4zm16 0h-3a3 3 0 0 0-3 3v14a3 3 0 0 1 3-3h3V4z", 13));

  const gearBtn = h("button", { title: tr("Settings"), onclick: () => go("settings") }, svg(ICONS.gear, 14));
  const soundBtn = h("button", { title: tr("Mute"), onclick: () => actions.toggleSound() }, svg(ICONS.speakerOn, 14));
  const pinBtn = h("button", { title: tr("Fixar painel"), "aria-label": tr("Fixar painel"), "aria-pressed": "false", onclick: () => actions.togglePin() },
    svg("M8 3h8l-1 6 3 4v2h-5v7h-2v-7H6v-2l3-4-1-6z", 13));

  function go(v: IslandViewName) {
    actions.blip();
    actions.setView(v);
  }

  const el = h(
    "div",
    { id: "header" },
    h("div", { class: "tabs" }, tabHome, tabChat, tabDrop, tabLearned),
    h("div", { class: "header-actions" }, pinBtn, gearBtn, soundBtn),
  );

  return {
    el,
    sync() {
      const v = State.view;
      tabHome.classList.toggle("on", v === "overview" || v === "empty");
      tabChat.classList.toggle("on", v === "prompt");
      tabDrop.classList.toggle("on", v === "upload");
      tabLearned.classList.toggle("on", v === "learned");
      gearBtn.classList.toggle("on", v === "settings");
      clear(gearBtn);
      gearBtn.append(svg(v === "settings" ? ICONS.gearFill : ICONS.gear, 14));
      clear(soundBtn);
      soundBtn.append(svg(State.settings.soundEnabled ? ICONS.speakerOn : ICONS.speakerOff, 14));
      soundBtn.title = State.settings.soundEnabled ? tr("Desativar som") : tr("Ativar som");
      pinBtn.classList.toggle("on", State.settings.userPinned);
      pinBtn.setAttribute("aria-pressed", String(State.settings.userPinned));
      pinBtn.title = State.settings.userPinned ? tr("Desafixar painel") : tr("Fixar painel");
      pinBtn.setAttribute("aria-label", pinBtn.title);
      el.style.opacity = v === "confused" ? "0" : "1";
    },
  };
}

// ── Overview ──────────────────────────────────────────────────────────────────

function buildOverview(actions: ViewActions): ViewHost {
  let ticker = new Ticker();
  const who = h("div", { class: "who" });
  const selector = h("select", { class: "session-selector", "aria-label": tr("Agent session"), onchange: () => {
    const agent = agentForTask(State.focusTask?.id ?? "");
    if (agent) State.selectSession(agent, selector.value);
  } });
  const tickerBody = h("div", { class: "card-body" }, who, selector, ticker.el);
  const leftBody = h("div", { class: "left-body" });
  const jump = h(
    "button",
    { class: "icon-btn jump", title: tr("Open"), onclick: () => actions.openTarget() },
    svg(ICONS.arrowUpRight, 8),
  );
  const left = card(null, leftBody, jump);
  const pills = h("div", { class: "pills" });
  const right = card(null, pills);

  const el = h("div", { class: "view overview" },
    h("div", { class: "left" }, left),
    h("div", { class: "right" }, right),
  );

  let pillIds = "";
  let detailOpen = false;
  let lastFocus: string | null = null;
  let mode: "ticker" | "card" | null = null;
  let cardKey = "";
  let sessionOptionsKey = "";
  let lastStep = "";

  const hooks: IntegrationCardHooks = {
    get detailOpen() {
      return detailOpen;
    },
    openDetail() {
      detailOpen = true;
      cardKey = "";
      State.notify();
    },
    closeDetail() {
      detailOpen = false;
      cardKey = "";
      State.notify();
    },
    openSettings: () => actions.openSettingsWindow(),
  };

  return {
    el,
    tick(nowMs: number) {
      if (mode === "ticker") ticker.tick(nowMs);
    },
    sync() {
      const task = State.focusTask;
      const metrics = aggregateAgentMetrics(AgentChat.conversations, State.sessions.list(), Math.floor(Date.now() / 1000));
      const focusIdentity = task ? `${task.id}:${task.sessionKey ?? ""}` : null;
      if (focusIdentity !== lastFocus) {
        lastFocus = focusIdentity;
        detailOpen = false;
        cardKey = "";
        mode = null;
        ticker = new Ticker();
        tickerBody.replaceChildren(who, selector, ticker.el);
        sessionOptionsKey = "";
        lastStep = "";
      }

      const agent = task ? agentForTask(task.id) : null;
      const sessionActive = agent != null && task?.sessionKey != null;

      if (task && sessionActive) {
        if (mode !== "ticker") {
          clear(leftBody);
          leftBody.append(tickerBody);
          mode = "ticker";
          cardKey = "";
        }
        clear(who);
        who.append(
          dot(task.color, 7),
          h("span", { class: "name", text: task.name }),
        );
        const badge = agentMetricsBadge(metrics[agent!]);
        if (badge) who.append(badge);
        who.append(h("button", { class: "metrics-open", title: tr("usage.title"), "aria-label": tr("usage.title"), text: "…", onclick: () => actions.setView("activity") }));
        const sessions = State.sessions.list(agent!);
        const optionsKey = sessions.map((session) => `${session.key}:${session.state}:${session.projectName}`).join("|");
        if (optionsKey !== sessionOptionsKey && document.activeElement !== selector) {
          sessionOptionsKey = optionsKey;
          selector.replaceChildren(...sessions.map((session) => h("option", {
            value: session.key,
            text: `${session.projectName} · ${session.sessionId.slice(-8)} · ${stateLabel(session.state, session.ended)}`,
          })));
        }
        selector.value = task.sessionKey!;
        selector.title = `${task.sessionCwd || tr("Session")}\n${task.sessionId || ""}`;
        // The bounded log rolls at 20 steps. Re-seed instead of showing an old
        // row forever when the rolling log's final index stays the same.
        const latest = task.steps.at(-1) ?? "";
        if (task.steps.length === 20 && lastStep !== latest) {
          ticker = new Ticker();
          tickerBody.replaceChildren(who, selector, ticker.el);
        }
        lastStep = latest;
        ticker.sync(task);
      } else if (task && agent) {
        const metric = metrics[agent];
        const availability = AgentChat.statuses.find((entry) => entry.agent === agent);
        const key = JSON.stringify([task.id, metric.badge, metric.awaitingApproval, availability?.available, availability?.detail]);
        if (cardKey !== key) {
          cardKey = key; mode = "card"; clear(leftBody);
          const badge = agentMetricsBadge(metric);
          const header = h("div", { class: "int-head" }, dot(task.color, 7), h("b", { text: task.name }));
          if (badge) header.append(badge);
          const label = metric.activeTotal ? formatText("overview.busySessions", { count: metric.activeTotal }) : metric.externalUnknown ? formatText("overview.externalUnknown", { count: metric.externalUnknown }) : availability?.available === false ? tr("chat.unavailable") : tr("state.idle");
          leftBody.append(h("div", { class: "int-card" }, header, h("div", { class: "int-status", text: label }), h("div", { class: "int-actions" }, h("button", { class: "link-btn", text: tr("overview.openChat"), onclick: () => {
            if (AgentChat.current?.approval) void denyAgentChatApproval(AgentChat.current.id);
            AgentChat.selectPersonal(agent);
            actions.setView("prompt");
          } }), h("button", { class: "link-btn", text: tr("usage.title"), onclick: () => actions.setView("activity") }))));
        }
      } else if (task) {
        const info = State.integrations[task.id];
        const key = [
          task.id, detailOpen, task.state, task.steps.join("|"),
          info?.loaded, info?.error, info?.configured,
          JSON.stringify(info?.data ?? {}),
        ].join("~");
        if (key !== cardKey) {
          cardKey = key;
          mode = "card";
          clear(leftBody);
          leftBody.append(renderIntegrationCard(task, hooks));
        }
      }

      jump.style.display = detailOpen ? "none" : "";

      const others = State.otherTasks;
      const pillKey = others.map((t) => {
        const agent = agentForTask(t.id);
        return `${t.id}:${t.pillBadge ?? ""}:${agent ? metrics[agent].badge : ""}:${agent ? metrics[agent].awaitingApproval : ""}`;
      }).join("|");
      if (pillKey !== pillIds) {
        pillIds = pillKey;
        clear(pills);
        for (const t of others) pills.append(buildPill(t, actions, agentForTask(t.id) ? metrics[agentForTask(t.id)!] : undefined));
        pruneMiniBots();
      }
    },
  };
}

function buildPill(task: AgentTask, actions: ViewActions, metrics?: AgentMetrics): HTMLElement {
  const agent = agentForTask(task.id);
  const label = task.name;
  const canvas = createMiniBot(task, 20);
  const pill = h(
    "div",
    { class: "pill", title: task.name, onclick: () => {
      actions.setFocus(task.id);
      if (agent) { if (AgentChat.current?.approval) void denyAgentChatApproval(AgentChat.current.id); AgentChat.selectPersonal(agent); actions.setView("prompt"); }
    } },
    canvas,
    h("span", { class: "lbl", text: label }),
  );
  if (agent && metrics) { const badge = agentMetricsBadge(metrics); if (badge) pill.append(badge); }
  pill.style.borderColor = `${task.color}24`;
  pill.addEventListener("mouseenter", () => {
    pill.style.background = `${task.color}2e`;
    pill.style.borderColor = `${task.color}8c`;
    pill.style.boxShadow = `0 2px 10px ${task.color}59`;
    (pill.querySelector(".lbl") as HTMLElement).style.color = lighten(task.color, 0.3);
  });
  pill.addEventListener("mouseleave", () => {
    pill.style.background = "";
    pill.style.borderColor = `${task.color}24`;
    pill.style.boxShadow = "";
    (pill.querySelector(".lbl") as HTMLElement).style.color = "";
  });

  if (task.pillBadge) {
    const colors = { approval: "#F5A524", finished: "#22C55E", error: "#F4505E" } as const;
    const icons = { approval: ICONS.bang, finished: ICONS.check, error: ICONS.xmark } as const;
    const inner = h("i", { style: `background:${colors[task.pillBadge]}` }, svg(icons[task.pillBadge], 6, { stroke: task.pillBadge === "finished" ? 3 : 0 }));
    const badge = h("div", { class: "pill-badge" }, inner);
    badge.style.boxShadow = `0 0 4px ${colors[task.pillBadge]}99`;
    pill.append(badge);
  }
  return pill;
}

function buildActivity(actions: ViewActions): ViewHost {
  const provider = h("select", { class: "chat-select", "aria-label": tr("chat.provider") });
  for (const agent of AGENT_IDS) provider.append(h("option", { value: agent, text: AGENT_META[agent].name }));
  const content = h("div", { class: "learned-list" });
  const el = h("div", { class: "view" }, card("indigo", h("div", { class: "chat-body" }, h("div", { class: "chat-controls" }, provider, h("button", { class: "chat-action", text: tr("overview.back"), onclick: () => actions.setView("overview") })), content)));
  let chosen: AgentId = "codex", previousFocus: string | null = null, key = "";
  provider.addEventListener("change", () => { chosen = provider.value as AgentId; key = ""; State.notify(); });
  function refresh() {
    if (State.view !== "activity") return;
    const focus = State.focusId;
    if (focus !== previousFocus) { previousFocus = focus; chosen = agentForTask(focus ?? "") ?? chosen; key = ""; }
    provider.value = chosen;
    const metric = aggregateAgentMetrics(AgentChat.conversations, State.sessions.list(), Math.floor(Date.now() / 1000))[chosen];
    const nextKey = JSON.stringify(metric);
    if (key === nextKey) return;
    key = nextKey; const scrollTop = content.scrollTop; clear(content);
    const panel = agentMetricsPanel(metric); panel.open = true; content.append(panel); content.scrollTop = scrollTop;
  }
  return { el, sync: refresh, tick: refresh };
}

function lighten(hex: string, amount: number): string {
  const v = parseInt(hex.replace("#", ""), 16);
  const c = [(v >> 16) & 255, (v >> 8) & 255, v & 255].map((x) =>
    Math.min(255, Math.round(x + amount * 255)),
  );
  return `rgb(${c[0]},${c[1]},${c[2]})`;
}

// ── Empty ─────────────────────────────────────────────────────────────────────

function buildEmpty(actions: ViewActions): ViewHost {
  const body = h(
    "div",
    { class: "stack", style: "padding:0 18px 0 118px;flex-direction:row;align-items:center;gap:16px" },
    h(
      "div",
      { style: "display:flex;flex-direction:column;gap:5px" },
      h("div", { class: "title", text: tr("Nothing running right now.") }),
      h("div", { class: "sub", text: tr("Drop a file or window, or ask me anything.") }),
    ),
    h("div", { class: "grow" }),
    btn(tr("Ask Claude"), "primary", () => actions.setView("prompt")),
  );
  return { el: h("div", { class: "view" }, card(null, body)), sync() {} };
}

// ── Approval ──────────────────────────────────────────────────────────────────

function buildApproval(actions: ViewActions): ViewHost {
  const who = h("div");
  const context = h("div", { class: "approval-context" });
  const code = h("div", { class: "code approval-command", tabindex: "0" });
  const row = h("div", { class: "actions" });
  const el = h("div", { class: "view" }, card("amber", stack(116, 16, who, context, code, row)));
  let rowKey = "";
  return {
    el,
    sync() {
      clear(who);
      const pendingTask = State.tasks.find((task) => task.id === State.pendingApproval?.taskId) ?? null;
      who.append(agentWho(pendingTask, tr("needs permission")));
      const session = State.sessions.get(State.pendingApproval?.sessionKey ?? "");
      context.textContent = `${session?.projectName ?? tr("Session")} · ${session?.sessionId.slice(-8) ?? ""}`;
      context.title = `${session?.cwd ?? ""}\n${session?.sessionId ?? ""}`;
      // The whole point of approving here rather than in the terminal: this line
      // is the command, the file path or the URL being authorised, not just the
      // name of the tool asking.
      code.textContent = State.pendingApproval?.command || State.pendingApproval?.tool || "…";
      code.title = code.textContent;
      // Two buttons, built once. Rebuilding them between a mouse-down and a
      // mouse-up would swallow the click, and there is nothing left to vary:
      // "Always" is gone until the remembered-rules list exists to back it.
      if (rowKey === "built") return;
      rowKey = "built";
      clear(row);
      row.append(
        btn(tr("Deny"), "secondary", () => actions.decide("deny"), "N"),
        btn(tr("Allow"), "primary", () => actions.decide("allow"), "Y"),
      );
    },
  };
}

// ── Question ──────────────────────────────────────────────────────────────────

function buildQuestion(): ViewHost {
  const who = h("div");
  const title = h("div", { class: "title" });
  const row = h("div", { class: "actions" });
  const el = h("div", { class: "view" }, card("cyan", stack(116, 16, who, title, row)));
  return {
    el,
    sync() {
      clear(who);
      who.append(agentWho(State.focusTask, tr("needs action")));
      const task = State.focusTask;
      title.textContent = task?.steps.at(-1) ?? formatText("template.needsAnswer", { agent: agentName(task) });
      clear(row);
      row.append(h("div", { class: "sub", text: tr("Answer in your terminal — Coucou can't reply for you yet.") }));
    },
  };
}

// ── Error ─────────────────────────────────────────────────────────────────────

function buildError(actions: ViewActions): ViewHost {
  const who = h("div");
  const title = h("div", { class: "title", text: tr("Workflow stopped.") });
  const detail = h("div", { class: "detail" });
  const row = h("div", { class: "actions" },
    btn(tr("Retry"), "primary", () => actions.setView(State.defaultView())),
    btn(tr("Open in n8n"), "secondary", () => actions.openUrl("")),
  );
  const el = h("div", { class: "view" }, card("red", stack(116, 16, who, title, detail, row)));
  return {
    el,
    sync() {
      const task = State.focusTask;
      clear(who);
      who.append(agentWho(task, agentName(task)));
      title.textContent = task?.source === "n8n" ? tr("Workflow stopped.") : tr("Session stopped on an error.");
      detail.textContent = task?.steps.at(-1) ?? tr("No detail available.");
    },
  };
}

// ── Finished ──────────────────────────────────────────────────────────────────

function buildFinished(actions: ViewActions): ViewHost {
  const who = h("div");
  const title = h("div", { class: "title" });
  const row = h("div", { class: "actions" },
    btn(tr("Open terminal"), "primary", () => actions.openTerminal()),
    btn("OK", "secondary", () => actions.collapse()),
  );
  const el = h("div", { class: "view" }, card("green", stack(116, 16, who, title, row)));
  return {
    el,
    sync() {
      clear(who);
      who.append(agentWho(State.focusTask, tr("status.finished")));
      title.textContent = State.focusTask?.steps.at(-1) ?? tr("Session finished");
    },
  };
}

// ── Confused ──────────────────────────────────────────────────────────────────

function buildConfused(): ViewHost {
  const body = h(
    "div",
    { class: "stack", style: "padding:0 18px 0 128px" },
    h("div", { class: "title", text: tr("Too many hits at once.") }),
    h("div", { class: "sub", text: tr("Give me a sec — back to work in three seconds.") }),
  );
  return { el: h("div", { class: "view" }, card("pink", body)), sync() {} };
}

// ── Note ──────────────────────────────────────────────────────────────────────

function buildNote(): ViewHost {
  const title = h("div", { class: "title" });
  const el = h("div", { class: "view" }, card(null, h("div", { class: "stack", style: "padding:0 18px 0 98px" }, title)));
  return {
    el,
    sync() {
      title.textContent = State.noteMessage ?? "";
    },
  };
}

// ── In-island settings ────────────────────────────────────────────────────────

function buildSettings(actions: ViewActions): ViewHost {
  const soundSwitch = h("button", { class: "switch", onclick: () => actions.toggleSound() });
  const volume = h("input", {
    type: "range", min: "0", max: "0.2", step: "0.005",
    oninput: (e: Event) => actions.setVolume(Number((e.target as HTMLInputElement).value)),
  }) as HTMLInputElement;
  const autoLabel = h("span", {});
  const segButtons = [10, 15, 30].map((s) =>
    h("button", { onclick: () => actions.setAutoClose(s) }, `${s}s`),
  );
  const claudeBadge = h("span", { class: "status-badge" });
  const apiBadge = h("span", { class: "status-badge" });

  const rows = h(
    "div",
    { class: "settings-rows" },
    h("div", { class: "settings-row" }, soundSwitch, h("span", { text: tr("Sound") }), volume),
    h(
      "div",
      { class: "settings-row" },
      svg(ICONS.timer, 12),
      autoLabel,
      h("div", { class: "seg" }, ...segButtons),
    ),
    h(
      "div",
      { class: "settings-row", style: "gap:14px" },
      claudeBadge,
      apiBadge,
      h("div", { class: "grow" }),
      h("button", {
        class: "link-btn",
        style: "color:#8e939c;font-size:11.5px",
        text: tr("Settings…"),
        onclick: () => actions.openSettingsWindow(),
      }),
    ),
  );

  const el = h("div", { class: "view" },
    card(null, h("div", { class: "stack", style: "padding:14px 16px 14px 84px" }, rows)));

  return {
    el,
    sync() {
      const s = State.settings;
      soundSwitch.classList.toggle("on", s.soundEnabled);
      volume.value = String(s.soundVolume);
      volume.style.opacity = s.soundEnabled ? "1" : "0.4";
      autoLabel.textContent = formatText("template.autoClose", { seconds: Math.round(s.autoCloseInterval) });
      segButtons.forEach((b, i) => b.classList.toggle("on", s.autoCloseInterval === [10, 15, 30][i]));
      clear(claudeBadge);
      claudeBadge.append(
        dot(s.hooksInstalled ? "#22C55E" : "#F4505E", 6),
        h("span", { text: "Claude Code" }),
      );
      clear(apiBadge);
      apiBadge.append(dot("#F4505E", 6), h("span", { text: "API" }));
    },
  };
}

// ── Placeholders filled in later stages ───────────────────────────────────────

function buildPlaceholder(title: string, sub: string): ViewHost {
  const body = h(
    "div",
    { class: "stack", style: "padding:0 18px 0 118px" },
    h("div", { class: "title", text: title }),
    h("div", { class: "sub", text: sub }),
  );
  return { el: h("div", { class: "view" }, card(null, body)), sync() {} };
}

// ── Registry ──────────────────────────────────────────────────────────────────

export function buildViews(
  actions: ViewActions,
  onChatHeightChange: () => void,
): Map<IslandViewName, ViewHost> {
  const map = new Map<IslandViewName, ViewHost>();
  map.set("overview", buildOverview(actions));
  map.set("empty", buildEmpty(actions));
  map.set("approval", buildApproval(actions));
  map.set("question", buildQuestion());
  map.set("error", buildError(actions));
  map.set("finished", buildFinished(actions));
  map.set("confused", buildConfused());
  map.set("note", buildNote());
  map.set("settings", buildSettings(actions));
  map.set("prompt", buildPrompt(onChatHeightChange, () => actions.setView("learned")));
  map.set("learned", buildLearned(actions));
  map.set("activity", buildActivity(actions));
  map.set("upload", buildUpload());
  map.set("uploading", buildUploading());
  map.set("choose", buildChoose(actions));
  // Not in the Windows v1: sending a file by email, window attach + web result.
  map.set("mail", buildPlaceholder(tr("Sending by email isn't in this version."), ""));
  map.set("searching", buildPlaceholder(tr("Claude is searching…"), ""));
  map.set("result", buildPlaceholder(tr("Result"), ""));
  return map;
}
