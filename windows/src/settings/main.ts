import { tr, formatText } from "../core/i18n";
// Settings window — the place where anything that writes to disk is confirmed.
// Stage 2 covers the Claude Code hooks and the general preferences; API keys and
// integrations land here too in a later stage.

import "./settings.css";
import { Bridge, onEvent, type HookStatus } from "../core/bridge";
import { DEFAULT_SETTINGS, type Settings } from "../core/state";
import { h, clear } from "../views/dom";

let settings: Settings = { ...DEFAULT_SETTINGS };
let version = "";
let syncGeneral = () => {};

const root = document.getElementById("settings-root")!;

async function save() {
  await Bridge.saveSettings(settings);
}

// ── Reusable bits ─────────────────────────────────────────────────────────────

function toggle(on: boolean, onChange: (v: boolean) => void): HTMLElement {
  const el = h("button", { class: on ? "switch on" : "switch", "aria-pressed": on });
  el.addEventListener("click", () => {
    const next = !el.classList.contains("on");
    el.classList.toggle("on", next);
    el.setAttribute("aria-pressed", String(next));
    onChange(next);
  });
  return el;
}

function statusDot(ok: boolean): HTMLElement {
  return h("i", { class: "dot", style: `background:${ok ? "#22c55e" : "#f4505e"}` });
}

function renderDiff(text: string): HTMLElement {
  const box = h("div", { class: "diff" });
  for (const line of text.split("\n")) {
    const cls = line.startsWith("+") ? "add" : line.startsWith("-") ? "del" : "ctx";
    box.append(h("div", { class: cls, text: line === "No change." ? tr("No change.") : line }));
  }
  return box;
}

// ── Local CLI agents ──────────────────────────────────────────────────────────

type CliAgent = "claude" | "codex" | "gemini" | "copilot";
interface CliAgentDefinition {
  agent: CliAgent;
  name: string;
  description: string;
  activation: string;
}
const CLI_AGENTS: CliAgentDefinition[] = [
  {
    agent: "claude", name: "Claude Code",
    description: tr("Follow local CLI sessions, tool activity and completion. Claude permission requests can be answered in the island."),
    activation: tr("Open a new Claude Code session to load these hooks."),
  },
  {
    agent: "codex", name: "Codex CLI",
    description: tr("Follow local CLI sessions, tool activity and completion. Permission decisions remain in Codex."),
    activation: tr("Open a new Codex CLI session, then use /hooks to review and trust Coucou's exact hook definitions. Coucou does not grant that trust."),
  },
  {
    agent: "gemini", name: "Gemini CLI",
    description: tr("Follow local CLI sessions, tool activity, completion and permission notifications. Answer permission requests in Gemini."),
    activation: tr("Restart Gemini CLI and check /hooks. If hooks are disabled by your settings or policy, enable them there after review."),
  },
  {
    agent: "copilot", name: "GitHub Copilot CLI",
    description: tr("Follow local CLI sessions, tool activity and completion. Permission decisions remain in Copilot. This integration uses a dedicated user hook file."),
    activation: tr("Restart Copilot CLI to load the user hooks. Repository hooks also run; avoid copying Coucou's handlers into a project hook file."),
  },
];

function agentSection(def: CliAgentDefinition, initial: HookStatus | null, initialError: string | null): HTMLElement {
  let status = initial;
  let error = initialError;
  const body = h("div", { style: "display:flex;flex-direction:column;gap:12px" });
  const heading = h("h2", {});
  const section = h("section", {}, heading, body);

  const rebuild = async () => {
    try {
      status = await Bridge.agentHooksStatus(def.agent);
      error = null;
    } catch (err) {
      error = String(err).replace(/^Error:\s*/, "");
    }
    clear(body);
    draw();
  };

  function draw() {
    clear(heading);
    heading.append(statusDot(status?.installed ?? false), h("span", { text: def.name }));
    body.append(h("div", { class: "hint", text: def.description }));
    if (error) {
      body.append(h("div", { class: "notice err", text: error }), h("div", { class: "row" }, h("button", { text: tr("Retry"), onclick: () => void rebuild() })));
      return;
    }
    if (!status) {
      body.append(h("div", { class: "notice warn", text: tr("Open settings inside Coucou to inspect and configure local CLI hooks.") }));
      return;
    }
    body.append(
      h("div", { class: "row" }, h("label", { text: "Hooks" }), h("span", { text: status.installed ? tr("Hooks present — open a CLI session to verify") : tr("Not configured") })),
      h("div", { class: "row" }, h("label", { text: tr("Configuration") }), h("span", { class: "path", text: status.settingsPath })),
      h("div", { class: "row" }, h("label", { text: tr("Relay") }), h("span", { class: "path", text: status.hookPath }), statusDot(status.hookReady)),
    );
    if (status.cliAvailable !== undefined) {
      body.append(h("div", { class: "row" }, h("label", { text: tr("CLI on PATH") }), h("span", { class: "hint", text: status.cliAvailable ? tr("Found — session compatibility still needs a real event") : tr("Not found — install the CLI or check the PATH used to launch Coucou") })));
    }
    body.append(h("div", { class: "hint", text: def.activation }));
    if (!status.hookReady) {
      body.append(h("div", { class: "notice warn", text: tr("coucou-hook.exe is unavailable. Restart Coucou before installing hooks.") }));
    }
    const install = h("button", { class: "primary", text: status.installed ? tr("Reinstall hooks…") : tr("Install hooks…"), onclick: () => void showPreview(true) });
    install.disabled = !status.hookReady;
    const actions = h("div", { class: "row" }, install, h("button", { text: tr("Refresh status"), onclick: () => void rebuild() }));
    if (status.installed) actions.append(h("button", { class: "danger", text: tr("Uninstall hooks…"), onclick: () => void showPreview(false) }));
    body.append(actions);
  }

  async function showPreview(install: boolean) {
    let preview;
    try {
      preview = await Bridge.agentHooksPreview(def.agent, install);
    } catch (err) {
      clear(body);
      body.append(h("div", { class: "notice err", text: String(err).replace(/^Error:\s*/, "") }), h("div", { class: "row" }, h("button", { text: tr("Back"), onclick: () => void rebuild() })));
      return;
    }
    if (!preview) return;
    clear(body);
    body.append(
      h("div", { class: "hint", text: install ? tr("Review the Coucou hooks below. Other settings and handlers are preserved. Nothing is written until you click the confirmation button.") : tr("Review removal of Coucou's handlers. Other settings and handlers are preserved.") }),
      h("div", { class: "row" }, h("span", { class: "path", text: preview.settingsPath })),
      renderDiff(preview.diff),
    );
    const noChange = preview.diff === "No change.";
    if (preview.backup) body.append(h("div", { class: "row" }, h("span", { class: "path", text: formatText("template.backup", { path: preview.backup }) })));
    else if (!noChange && install) body.append(h("div", { class: "hint", text: tr("This creates a new configuration file; there is no previous file to back up.") }));
    if (noChange) {
      body.append(h("div", { class: "row" }, h("button", { text: tr("Back"), onclick: () => void rebuild() })));
      return;
    }
    const confirm = h("button", { class: install ? "primary" : "danger", text: install ? tr("Apply reviewed hooks") : tr("Remove reviewed hooks") });
    confirm.addEventListener("click", async () => {
      confirm.disabled = true;
      try {
        const backup = await Bridge.agentHooksApply(def.agent, install, preview.fingerprint);
        clear(body);
        body.append(h("div", { class: "notice ok", text: `${install ? tr("Hooks configured.") : tr("Coucou hooks removed.")}${backup ? formatText("template.previousBytes", { path: backup }) : ""} ${def.activation}` }), h("div", { class: "row" }, h("button", { text: tr("Back"), onclick: () => void rebuild() })));
      } catch (err) {
        // A stale preview must be reviewed again instead of retrying its token.
        clear(body);
        body.append(h("div", { class: "notice err", text: formatText("template.applyError", { error: String(err).replace(/^Error:\s*/, "") }) }), h("div", { class: "row" }, h("button", { text: tr("Review again"), onclick: () => void showPreview(install) }), h("button", { text: tr("Back"), onclick: () => void rebuild() })));
      }
    });
    body.append(h("div", { class: "row" }, confirm, h("button", { text: tr("Cancel"), onclick: () => void rebuild() })));
  }

  draw();
  return section;
}

// ── Claude API section ────────────────────────────────────────────────────────

const MODELS: [string, string][] = [
  ["claude-opus-5", "Claude Opus 5"],
  ["claude-sonnet-5", "Claude Sonnet 5"],
  ["claude-haiku-4-5", "Claude Haiku 4.5"],
];

function apiSection(hasKey: boolean): HTMLElement {
  const dot = statusDot(hasKey);
  const state = h("span", { class: "hint", text: hasKey ? tr("Key saved in the Windows Credential Manager.") : tr("No API key yet — only the Anthropic API chat option needs one.") });

  const field = h("input", {
    type: "password",
    placeholder: hasKey ? tr("••••••••••••  (stored)") : "sk-ant-...",
    style: "flex:1 1 auto;min-width:0",
    autocomplete: "off",
    spellcheck: "false",
  }) as HTMLInputElement;

  const saveBtn = h("button", { class: "primary", text: tr("Save key") });
  const clearBtn = h("button", { class: "danger", text: tr("Remove") });
  const feedback = h("div", {});

  async function refresh() {
    const present = (await Bridge.secretPresent("anthropic-api-key")) ?? false;
    dot.style.background = present ? "#22c55e" : "#f4505e";
    state.textContent = present
      ? tr("Key saved in the Windows Credential Manager.")
      : tr("No API key yet — only the Anthropic API chat option needs one.");
    field.placeholder = present ? tr("••••••••••••  (stored)") : "sk-ant-...";
    clearBtn.style.display = present ? "" : "none";
  }

  saveBtn.addEventListener("click", async () => {
    const value = field.value.trim();
    if (!value) return;
    clear(feedback);
    try {
      await Bridge.secretSet("anthropic-api-key", value);
      field.value = "";
      feedback.append(h("div", { class: "notice ok", text: tr("Saved. It never touches disk.") }));
      await refresh();
    } catch (err) {
      feedback.append(h("div", { class: "notice err", text: formatText("template.saveError", { error: String(err) }) }));
    }
  });

  clearBtn.addEventListener("click", async () => {
    clear(feedback);
    try {
      await Bridge.secretClear("anthropic-api-key");
      feedback.append(h("div", { class: "notice ok", text: tr("Key removed.") }));
      await refresh();
    } catch (err) {
      feedback.append(h("div", { class: "notice err", text: formatText("template.removeError", { error: String(err) }) }));
    }
  });

  const model = h("select", {}) as HTMLSelectElement;
  for (const [id, label] of MODELS) model.append(h("option", { value: id, text: label }));
  if (!MODELS.some(([id]) => id === settings.model)) {
    model.append(h("option", { value: settings.model, text: settings.model }));
  }
  model.value = settings.model;
  model.addEventListener("change", () => {
    settings.model = model.value;
    void save();
  });

  clearBtn.style.display = hasKey ? "" : "none";

  return h(
    "section",
    {},
    h("h2", {}, dot, h("span", { text: tr("Anthropic API chat") })),
    state,
    h("div", { class: "row" }, h("label", { text: tr("API key") }), field, saveBtn, clearBtn),
    h("div", { class: "row" }, h("label", { text: tr("Model") }), model),
    feedback,
  );
}

// ── Integrations section ──────────────────────────────────────────────────────

interface IntegrationDef {
  id: string;
  name: string;
  color: string;
  /** Credential Manager keys, in the order they are shown. */
  fields: { key: string; label: string; placeholder: string; secret: boolean }[];
}

const INTEGRATIONS: IntegrationDef[] = [
  { id: "integration_stripe", name: "Stripe", color: "#0570DE",
    fields: [{ key: "stripe-api-key", label: tr("Secret key"), placeholder: "sk_live_…", secret: true }] },
  { id: "integration_github", name: "GitHub", color: "#F4505E",
    fields: [{ key: "github-token", label: "Token", placeholder: "ghp_…", secret: true }] },
  { id: "integration_vercel", name: "Vercel", color: "#7C5CFF",
    fields: [{ key: "vercel-token", label: "Token", placeholder: "…", secret: true }] },
  { id: "integration_n8n", name: "n8n", color: "#F29B38",
    fields: [
      { key: "n8n-url", label: tr("Instance URL"), placeholder: "https://n8n.example.com", secret: false },
      { key: "n8n-api-key", label: tr("API key"), placeholder: "…", secret: true },
    ] },
  { id: "integration_resend", name: "Resend", color: "#22C55E",
    fields: [{ key: "resend-api-key", label: tr("API key"), placeholder: "re_…", secret: true }] },
  { id: "integration_notion", name: "Notion", color: "#8C8C8C",
    fields: [{ key: "notion-api-key", label: tr("Integration token"), placeholder: "ntn_…", secret: true }] },
  { id: "integration_calcom", name: "Cal.com", color: "#C9956A",
    fields: [{ key: "calcom-api-key", label: tr("API key"), placeholder: "cal_…", secret: true }] },
];

const MAX_ACTIVE = 4;

function integrationsSection(present: Record<string, boolean>): HTMLElement {
  const note = h("div", { class: "hint" });
  const list = h("div", { style: "display:flex;flex-direction:column;gap:14px" });

  function updateNote() {
    const used = settings.activeIntegrations.length;
    note.textContent = formatText("template.integrationLimit", { max: MAX_ACTIVE, used });
  }

  for (const def of INTEGRATIONS) {
    const active = settings.activeIntegrations.includes(def.id);
    const sw = h("button", { class: active ? "switch on" : "switch" });
    sw.addEventListener("click", () => {
      const on = settings.activeIntegrations.includes(def.id);
      if (on) {
        settings.activeIntegrations = settings.activeIntegrations.filter((x) => x !== def.id);
      } else {
        if (settings.activeIntegrations.length >= MAX_ACTIVE) return;
        settings.activeIntegrations = [...settings.activeIntegrations, def.id];
      }
      sw.classList.toggle("on", !on);
      updateNote();
      void save();
    });

    const rows = h("div", { style: "display:flex;flex-direction:column;gap:6px;flex:1 1 auto;min-width:0" });
    for (const field of def.fields) {
      const input = h("input", {
        type: field.secret ? "password" : "text",
        placeholder: present[field.key] ? tr("••••••••  (stored)") : field.placeholder,
        autocomplete: "off",
        spellcheck: "false",
        style: "flex:1 1 auto;min-width:0",
      }) as HTMLInputElement;
      const saveBtn = h("button", { text: tr("Save") });
      const dotEl = statusDot(present[field.key] ?? false);
      saveBtn.addEventListener("click", async () => {
        const value = input.value.trim();
        try {
          await Bridge.secretSet(field.key, value);
          present[field.key] = value.length > 0;
          input.value = "";
          input.placeholder = value ? tr("••••••••  (stored)") : field.placeholder;
          dotEl.style.background = value ? "#22c55e" : "#f4505e";
        } catch {
          dotEl.style.background = "#f5a524";
        }
      });
      rows.append(
        h("div", { class: "row" },
          h("label", { style: "min-width:104px", text: field.label }),
          input, saveBtn, dotEl,
        ),
      );
    }

    list.append(
      h("div", { style: "display:flex;gap:12px;align-items:flex-start" },
        h("div", { style: "display:flex;align-items:center;gap:8px;min-width:132px;padding-top:4px" },
          sw,
          h("i", { class: "dot", style: `background:${def.color}` }),
          h("span", { style: "font-size:12.5px", text: def.name }),
        ),
        rows,
      ),
    );
  }

  updateNote();
  return h("section", {}, h("h2", {}, h("span", { text: tr("Integrations") })), note, list);
}

// ── General section ───────────────────────────────────────────────────────────

function generalSection(): HTMLElement {
  const visibility = h("select", { "aria-label": tr("visibility.label") }) as HTMLSelectElement;
  visibility.append(
    h("option", { value: "always", text: tr("visibility.always") }),
    h("option", { value: "autoHide", text: tr("visibility.autoHide") }),
  );
  visibility.value = settings.visibilityMode;
  visibility.addEventListener("change", () => {
    settings.visibilityMode = visibility.value === "autoHide" ? "autoHide" : "always";
    void save();
  });
  const pinned = toggle(settings.userPinned, (value) => { settings.userPinned = value; void save(); });
  pinned.setAttribute("aria-label", tr("pin.label"));
  const volume = h("input", {
    type: "range", min: "0", max: "0.2", step: "0.005",
    value: String(settings.soundVolume),
  }) as HTMLInputElement;
  volume.addEventListener("input", () => {
    settings.soundVolume = Number(volume.value);
    void save();
  });

  const autoClose = h("input", {
    type: "number", min: "5", max: "120", step: "1",
    value: String(Math.round(settings.autoCloseInterval)),
    style: "width:72px",
  }) as HTMLInputElement;
  autoClose.addEventListener("change", () => {
    settings.autoCloseInterval = Math.max(5, Math.min(120, Number(autoClose.value) || 15));
    autoClose.value = String(settings.autoCloseInterval);
    void save();
  });

  const screen = h("select", {}) as HTMLSelectElement;
  screen.append(
    h("option", { value: "primary", text: tr("Main display") }),
    h("option", { value: "cursor", text: tr("Display under the cursor") }),
  );
  screen.value = settings.screen;
  screen.addEventListener("change", () => {
    settings.screen = screen.value as Settings["screen"];
    void save();
  });
  syncGeneral = () => {
    visibility.value = settings.visibilityMode;
    pinned.classList.toggle("on", settings.userPinned);
    pinned.setAttribute("aria-pressed", String(settings.userPinned));
  };

  return h(
    "section",
    {},
    h("h2", {}, h("span", { text: tr("General") })),
    h("div", { class: "row" }, h("label", { text: tr("visibility.label") }), visibility),
    h("div", { class: "hint", text: tr("visibility.hint") }),
    h("div", { class: "row" }, h("label", { text: tr("pin.label") }), pinned),
    h("div", { class: "hint", text: tr("pin.hint") }),
    h("div", { class: "row" },
      h("label", { text: tr("Sound") }),
      toggle(settings.soundEnabled, (v) => { settings.soundEnabled = v; void save(); }),
      volume,
    ),
    h("div", { class: "row" },
      h("label", { text: tr("Auto-close") }),
      autoClose,
      h("span", { class: "hint", text: tr("seconds after you leave the island") }),
    ),
    h("div", { class: "row" },
      h("label", { text: tr("Island lives on") }),
      screen,
    ),
    h("div", { class: "row" },
      h("label", { text: tr("Launch at startup") }),
      toggle(settings.autostart, (v) => { settings.autostart = v; void save(); }),
    ),
  );
}

// ── Boot ──────────────────────────────────────────────────────────────────────

async function main() {
  const boot = await Bridge.boot();
  if (boot) {
    settings = { ...settings, ...boot.settings };
    version = boot.version;
  }
  const agentStates = await Promise.all(CLI_AGENTS.map(async (def) => {
    try { return { status: await Bridge.agentHooksStatus(def.agent), error: null }; }
    catch (err) { return { status: null, error: String(err).replace(/^Error:\s*/, "") }; }
  }));

  const hasKey = (await Bridge.secretPresent("anthropic-api-key")) ?? false;

  const keys = [
    "stripe-api-key", "github-token", "vercel-token",
    "n8n-url", "n8n-api-key", "resend-api-key", "notion-api-key", "calcom-api-key",
  ];
  const present: Record<string, boolean> = {};
  for (const k of keys) present[k] = (await Bridge.secretPresent(k)) ?? false;

  clear(root);
  root.append(
    h("h1", {}, h("span", { text: "Coucou" }), h("span", { class: "version", text: version })),
    h("section", {}, h("h2", {}, h("span", { text: tr("Chat through local CLIs") })), h("div", { class: "hint", text: tr("Choose Codex, Claude Code, Gemini or Copilot in the island chat, then enter the project folder. Each CLI uses its own existing login and usage limits. CLI chat does not need the Anthropic API key or activity hooks. Start in read-only mode; allowing changes is an explicit choice per conversation.") })),
    ...CLI_AGENTS.map((def, index) => agentSection(def, agentStates[index].status, agentStates[index].error)),
    apiSection(hasKey),
    integrationsSection(present),
    generalSection(),
    h("div", {
      class: "hint",
      text: tr("No telemetry. Network requests only go to the services you configure yourself."),
    }),
  );

  void onEvent<Settings>("settings-changed", (s) => {
    settings = { ...settings, ...s };
    syncGeneral();
  });
}

void main();
