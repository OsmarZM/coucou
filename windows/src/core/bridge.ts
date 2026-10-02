// Thin wrapper over the Tauri commands/events. Every call is a no-op when the
// page is opened in a plain browser, so the island can be iterated on with
// `npm run dev` alone.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import type { Settings } from "./state";
import type { Attachment, DocumentChunk, PreparedDocuments } from "./document-types";
import type { MemoryPreferences, MemoryQuery, MemoryCandidate, MemoryRecord, SkillCandidate, SkillRecord, SkillDiff, SkillExport, SessionRecord, StoredMessage, SessionSearchHit } from "./personal-types";

export const IS_TAURI =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T | null> {
  if (!IS_TAURI) return null;
  try {
    return await invoke<T>(cmd, args);
  } catch (err) {
    console.error(`[coucou] ${cmd} failed`, err);
    return null;
  }
}

export interface BootInfo {
  settings: Settings;
  /** Logical screen rect of the monitor the island lives on. */
  screen: { x: number; y: number; width: number; height: number; scale: number };
  version: string;
  hookPath: string;
}

export const Bridge = {
  boot: () => call<BootInfo>("boot"),

  saveSettings: (settings: Settings) => call<void>("save_settings", { settings }),

  /** Shrink the window down to the invisible wake strip (hidden) or back to full. */
  setCollapsed: (collapsed: boolean) => call<void>("set_collapsed", { collapsed }),

  /**
   * Pushes the island shape in window coordinates. Rust flips click-through from
   * its own cursor poll, so the flag is never a frame behind a click.
   */
  setIslandRect: (x: number, y: number, width: number, height: number) =>
    call<void>("set_island_rect", { x, y, width, height }),

  /** Give the window keyboard focus (chat field) and take it away again. */
  focusWindow: (focused: boolean) => call<void>("focus_window", { focused }),

  reposition: () => call<void>("reposition"),

  openUrl: (url: string) => call<void>("open_url", { url }),

  /** "Open terminal" → opens the folder in VS Code when `code` is on PATH. */
  openInVSCode: (path: string | null) => call<boolean>("open_in_vscode", { path }),

  quit: () => call<void>("quit_app"),

  openSettingsWindow: () => call<void>("open_settings_window"),

  /** Writes to %LOCALAPPDATA%\Coucou\coucou.log, next to the Rust lines. */
  log: (message: string) => call<void>("log_line", { message }),

  // ── Claude Code hooks ─────────────────────────────────────────────────────
  hooksStatus: () => call<HookStatus>("hooks_status"),
  /** Diff to show before anything is written. `install: false` previews removal. */
  hooksPreview: (install: boolean) => callOrThrow<HookPreview>("hooks_preview", { install }),
  /**
   * Writes ~/.claude/settings.json — only ever after an explicit click, and only
   * when the file still matches the preview the user looked at.
   */
  hooksApply: (install: boolean, fingerprint: string) =>
    callOrThrow<string>("hooks_apply", { install, fingerprint }),

  agentHooksStatus: (agent: string) => IS_TAURI
    ? callOrThrow<HookStatus>("agent_hooks_status", { agent })
    : Promise.resolve(null),
  agentHooksPreview: (agent: string, install: boolean) =>
    callOrThrow<HookPreview>("agent_hooks_preview", { agent, install }),
  agentHooksApply: (agent: string, install: boolean, fingerprint: string) =>
    callOrThrow<string>("agent_hooks_apply", { agent, install, fingerprint }),

  approvalDecision: (requestId: string, decision: "allow" | "deny") =>
    callOrThrow<boolean>("approval_decision", { requestId, decision }),
  /** "The card is up" — until this lands the relay only waits a moment. */
  approvalAck: (requestId: string) => call<void>("approval_ack", { requestId }),
  /** "Nobody can act on this" — Claude Code asks in the terminal right away. */
  approvalDecline: (requestId: string) => call<void>("approval_decline", { requestId }),

  // ── Chat, files, secrets ──────────────────────────────────────────────────
  /** One chat turn. The API key and any file bytes never leave Rust. */
  chatSend: (query: string, context: ChatContext | null) =>
    callOrThrow<{ text: string }>("chat_send", { query, context }),
  chatReset: () => call<void>("chat_reset"),
  agentChatStatus: () => callOrThrow<AgentChatStatus[]>("agent_chat_status"),
  agentChatStart: (request: AgentChatStart) => callOrThrow<void>("agent_chat_start", { request }),
  agentChatCancel: (conversationId: string, runId: string) =>
    callOrThrow<boolean>("agent_chat_cancel", { conversationId, runId }),
  agentChatDecide: (conversationId: string, runId: string, requestId: string, decision: "allow" | "allowConversation" | "deny") =>
    callOrThrow<boolean>("agent_chat_decide", { conversationId, runId, requestId, decision }),
  memoryPreferences: () => callOrThrow<MemoryPreferences>("memory_preferences"),
  memorySetPreferences: (preferences: MemoryPreferences) => callOrThrow<MemoryPreferences>("memory_set_preferences", { preferences }),
  memoryList: (query: MemoryQuery) => callOrThrow<MemoryRecord[]>("memory_list", { query }),
  memoryPropose: (candidate: MemoryCandidate) => callOrThrow<MemoryRecord>("memory_propose", { candidate }),
  memoryApprove: (id: string, revision: number) => callOrThrow<MemoryRecord>("memory_approve", { id, revision }),
  memoryReject: (id: string, revision: number) => callOrThrow<MemoryRecord>("memory_reject", { id, revision }),
  memoryForget: (id: string, revision: number) => callOrThrow<number>("memory_forget", { id, revision }),
  memoryExport: (includeHistory: boolean) => callOrThrow<string>("memory_export", { includeHistory }),
  skillsList: (query: MemoryQuery) => callOrThrow<SkillRecord[]>("skills_list", { query }),
  skillsPropose: (candidate: SkillCandidate) => callOrThrow<SkillRecord>("skills_propose", { candidate }),
  skillsApprove: (id: string, revision: number) => callOrThrow<SkillRecord>("skills_approve", { id, revision }),
  skillsReject: (id: string, revision: number) => callOrThrow<SkillRecord>("skills_reject", { id, revision }),
  skillsForget: (id: string, revision: number) => callOrThrow<number>("skills_forget", { id, revision }),
  skillsDiff: (id: string, revision: number) => callOrThrow<SkillDiff>("skills_diff", { id, revision }),
  skillsLoad: (id: string, revision: number) => callOrThrow<SkillRecord>("skills_load", { id, revision }),
  skillsExport: (id: string, revision: number) => callOrThrow<SkillExport>("skills_export", { id, revision }),
  skillsImport: (markdown: string, scope: string) => callOrThrow<SkillRecord>("skills_import", { markdown, scope }),
  skillsRestore: (id: string) => callOrThrow<SkillRecord>("skills_restore", { id }),
  historyList: (limit = 100) => callOrThrow<SessionRecord[]>("history_list", { limit }),
  historySearch: (query: string, limit = 50) => callOrThrow<SessionSearchHit[]>("history_search", { query, limit }),
  historyMessages: (conversationId: string, limit = 100) => callOrThrow<StoredMessage[]>("history_messages", { conversationId, limit }),
  historyContext: (contextId: string, limit = 100, budget = 65536) => callOrThrow<import("./personal-types").PersonalContextHistory>("history_context", { contextId, limit, budget }),
  historyForget: (conversationId: string) => callOrThrow<number>("history_forget", { conversationId }),
  permissionsRevoke: (conversationId: string) => callOrThrow<void>("permissions_revoke", { conversationId }),
  documentsChoose: () => callOrThrow<string[]>("documents_choose"),
  documentsIngest: (conversationId: string, paths: string[]) => callOrThrow<Attachment[]>("documents_ingest", { conversationId, paths }),
  documentsList: (conversationId: string) => callOrThrow<Attachment[]>("documents_list", { conversationId }),
  documentsRemove: (conversationId: string, id: string) => callOrThrow<void>("documents_remove", { conversationId, id }),
  documentsPrepare: (conversationId: string, attachmentIds: string[], budgetChars: number) => callOrThrow<PreparedDocuments>("documents_prepare", { conversationId, attachmentIds, budgetChars }),
  documentsRead: (conversationId: string, id: string, offsetChars: number, limitChars: number) => callOrThrow<DocumentChunk>("documents_read", { conversationId, id, offsetChars, limitChars }),
  /** Copies a dropped file into the inbox. */
  ingestFile: (path: string) => callOrThrow<DroppedFile>("ingest_file", { path }),
  /** Only ever tells you whether a key exists — never its value. */
  secretPresent: (key: string) => call<boolean>("secret_present", { key }),
  secretSet: (key: string, value: string) => callOrThrow<void>("secret_set", { key, value }),
  secretClear: (key: string) => callOrThrow<void>("secret_clear", { key }),

  // ── Integrations ──────────────────────────────────────────────────────────
  refreshIntegration: (id: string) => call<void>("refresh_integration", { id }),
  /** Opens the configured n8n instance in the browser. */
  openN8n: () => call<void>("open_n8n"),

  /** Tray → Pause. Stops the integration pollers, not just the island. */
  setPaused: (paused: boolean) => call<void>("set_paused", { paused }),
};

export interface IntegrationUpdate {
  id: string;
  data: Record<string, unknown>;
  error: string | null;
  event: { success: boolean; label: string; detail: string | null } | null;
}

export type ChatAgent = "codex" | "claude" | "gemini" | "copilot";
export interface AgentChatStatus {
  agent: ChatAgent;
  available: boolean;
  path: string | null;
  detail: string;
  readonlyOnly?: boolean;
  personalSupported?: boolean;
}
export interface AgentChatStart {
  conversationId: string;
  runId: string;
  agent: ChatAgent;
  cwd: string;
  sessionId: string | null;
  query: string;
  writable: boolean;
  attachmentIds: string[];
  contextId?: string | null;
}
export interface AgentChatEvent {
  conversationId: string;
  runId: string;
  kind: string;
  data: Record<string, unknown>;
}

export type ChatContext =
  | { kind: "file"; name: string; path: string }
  | { kind: "window"; appName: string; title: string; url?: string };

export interface DroppedFile {
  name: string;
  path: string;
  size: number;
}

export interface HookStatus {
  agent?: string;
  cliAvailable?: boolean;
  installed: boolean;
  settingsPath: string;
  hookPath: string;
  hookReady: boolean;
}

export interface HookPreview {
  diff: string;
  backup: string;
  settingsPath: string;
  /** Hand back to hooksApply so only the reviewed diff is ever written. */
  fingerprint: string;
}

/** Same as `call`, but surfaces the error so the UI can show what went wrong. */
async function callOrThrow<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!IS_TAURI) throw new Error("not running inside Coucou");
  return invoke<T>(cmd, args);
}

export type BridgeEvent =
  | { name: "cursor"; payload: { x: number; y: number } }
  | { name: "tray"; payload: string }
  | { name: "hook"; payload: Record<string, unknown> }
  | { name: "screen-changed"; payload: null };

export interface DragDropPayload {
  type: "enter" | "over" | "drop" | "leave";
  paths?: string[];
}

/** Files dragged onto the island. Only reaches us when the window takes the mouse. */
export async function onDragDrop(handler: (e: DragDropPayload) => void) {
  if (!IS_TAURI) return () => {};
  return getCurrentWebview().onDragDropEvent((event) => {
    handler(event.payload as DragDropPayload);
  });
}

export async function onEvent<T>(name: string, handler: (payload: T) => void) {
  if (!IS_TAURI) return () => {};
  return listen<T>(name, (e) => handler(e.payload));
}
