import { Bridge, type AgentChatEvent } from "./bridge";
import { AgentChat, chatDiagnostic, observeAgentChatEvents } from "./agent-chat";
import { State } from "./state";
import { tr } from "./i18n";
import type { Attachment } from "./document-types";

export interface ConversationAttachments { items: Attachment[]; selected: Set<string>; busy: number; loaded: boolean; error: string | null; generation: number; preparation: { usedChars: number; budgetChars: number; partial: boolean } | null }
interface DocumentBridge { documentsList(id: string): Promise<Attachment[]>; documentsIngest(id: string, paths: string[]): Promise<Attachment[]>; documentsRemove(id: string, attachmentId: string): Promise<void> }
/** Every asynchronous operation captures its conversation, never the current tab. */
export class AttachmentStore {
  private entries = new Map<string, ConversationAttachments>();
  private queues = new Map<string, Promise<unknown>>();
  constructor(private bridge: DocumentBridge = Bridge, private changed: () => void = () => {}, private busyChanged: (busy: boolean) => void = () => {}, private scope: (conversationId: string) => string = (id) => id) {}
  get(conversationId: string) {
    conversationId = this.scope(conversationId);
    let entry = this.entries.get(conversationId);
    if (!entry) {
      if (this.entries.size >= 16) {
        const oldest = [...this.entries].find(([, value]) => !value.busy);
        if (oldest) this.entries.delete(oldest[0]);
      }
      entry = { items: [], selected: new Set(), busy: 0, loaded: false, error: null, generation: 0, preparation: null };
      this.entries.set(conversationId, entry);
    }
    return entry;
  }
  selectedIds(conversationId: string) {
    const entry = this.get(conversationId);
    return entry.items.filter((item) => entry.selected.has(item.id) && ["queued", "ready"].includes(item.status)).map((item) => item.id);
  }
  toggle(conversationId: string, id: string, selected: boolean) {
    const entry = this.get(conversationId);
    if (!entry.items.some((item) => item.id === id && ["queued", "ready"].includes(item.status))) return;
    if (selected) entry.selected.add(id); else entry.selected.delete(id);
    this.changed();
  }
  private replace(conversationId: string, items: Attachment[], selectNew: boolean) {
    conversationId = this.scope(conversationId);
    const entry = this.get(conversationId);
    const previous = new Set(entry.items.map((item) => item.id));
    entry.items = items.filter((item) => item.conversationId === conversationId).slice(0, 10);
    entry.selected = new Set([...entry.selected].filter((id) => entry.items.some((item) => item.id === id && ["queued", "ready"].includes(item.status))));
    if (selectNew) for (const item of entry.items) if (!previous.has(item.id) && ["queued", "ready"].includes(item.status)) entry.selected.add(item.id);
    entry.loaded = true;
  }
  async refresh(conversationId: string) {
    conversationId = this.scope(conversationId);
    const entry = this.get(conversationId);
    const generation = ++entry.generation;
    try {
      const items = await this.bridge.documentsList(conversationId);
      if (generation !== entry.generation) return;
      this.replace(conversationId, items, !entry.loaded);
      entry.error = null;
    } catch (error) { if (generation === entry.generation) entry.error = chatDiagnostic(error); }
    this.changed();
  }
  ensureLoaded(conversationId: string) {
    const entry = this.get(conversationId);
    if (entry.loaded || entry.busy) return;
    // Mark before awaiting, so frame-based sync cannot dispatch duplicate reads.
    entry.loaded = true;
    void this.refresh(conversationId);
  }
  private enqueue<T>(conversationId: string, operation: () => Promise<T>): Promise<T> {
    const entry = this.get(conversationId);
    entry.busy++; entry.generation++; entry.error = null;
    this.busyChanged(true); this.changed();
    const previous = this.queues.get(conversationId) ?? Promise.resolve();
    const result = previous.catch(() => {}).then(operation);
    this.queues.set(conversationId, result);
    return result.catch((error) => { entry.error = chatDiagnostic(error); throw error; }).finally(() => {
      entry.busy = Math.max(0, entry.busy - 1);
      if (this.queues.get(conversationId) === result) this.queues.delete(conversationId);
      this.busyChanged([...this.entries.values()].some((value) => value.busy > 0)); this.changed();
    });
  }
  ingest(conversationId: string, paths: string[]) {
    conversationId = this.scope(conversationId);
    if (!paths.length || paths.length > 10) return Promise.reject(new Error(tr("chat.attachmentLimit")));
    const captured = [...paths];
    return this.enqueue(conversationId, async () => {
      const entry = this.get(conversationId);
      if (entry.items.length + captured.length > 10) throw new Error(tr("chat.attachmentLimit"));
      const added = await this.bridge.documentsIngest(conversationId, captured);
      this.replace(conversationId, [...entry.items, ...added], true);
      return added;
    });
  }
  remove(conversationId: string, id: string) {
    conversationId = this.scope(conversationId);
    return this.enqueue(conversationId, async () => {
      await this.bridge.documentsRemove(conversationId, id);
      const entry = this.get(conversationId);
      this.replace(conversationId, entry.items.filter((item) => item.id !== id), false);
    });
  }
  applyPrepared(event: AgentChatEvent) {
    if (event.kind !== "attachments" || !Array.isArray(event.data.attachments)) return;
    const conversationId = this.scope(event.conversationId);
    const entry = this.get(conversationId);
    entry.generation++; // A previously dispatched list cannot replace prepared coverage.
    const merged = new Map(entry.items.map((item) => [item.id, item]));
    for (const item of event.data.attachments as Attachment[]) if (item.conversationId === conversationId) merged.set(item.id, item);
    this.replace(conversationId, [...merged.values()], false);
    if (typeof event.data.usedChars === "number" && typeof event.data.budgetChars === "number") entry.preparation = { usedChars: event.data.usedChars, budgetChars: event.data.budgetChars, partial: event.data.partial === true };
    this.changed();
  }
  forget(conversationId: string) { if (this.scope(conversationId) !== conversationId) return; this.entries.delete(conversationId); this.changed(); }
}

export const attachmentScope = (id: string) => { const conversation = AgentChat.conversations.find((entry) => entry.id === id); return conversation?.contextId ?? id; };
export const attachmentScopeBusy = (id: string) => AgentChat.conversations.some((entry) => !!entry.runId && (entry.contextId ?? entry.id) === attachmentScope(id));
export const Attachments = new AttachmentStore(Bridge, () => State.notify(), (busy) => { State.attachmentBusy = busy; }, attachmentScope);
observeAgentChatEvents((event) => Attachments.applyPrepared(event));
let conversationResolver: (() => ReturnType<typeof AgentChat.create>) | null = null;
export function setAttachmentConversationResolver(resolver: () => ReturnType<typeof AgentChat.create>) { conversationResolver = resolver; }
export function ensureAttachmentConversation() {
  if (AgentChat.provider === "anthropic") throw new Error(tr("chat.apiAttachments"));
  if (conversationResolver) return conversationResolver();
  const current = AgentChat.current;
  return current?.agent === AgentChat.provider ? current : AgentChat.create({ agent: AgentChat.provider, cwd: "", writable: false });
}
export async function ingestConversationAttachments(conversationId: string, paths: string[]) {
  const conversation = AgentChat.conversations.find((entry) => entry.id === conversationId);
  if (conversation && attachmentScopeBusy(conversationId)) throw new Error(tr("chat.attachReadonly"));
  return Attachments.ingest(conversationId, paths);
}
