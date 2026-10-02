import { Bridge } from "./bridge";
import { AgentChat, chatDiagnostic, cancelAgentChat, observeAgentChatEvents } from "./agent-chat";
import { Attachments } from "./attachments";
import { State } from "./state";
import type { MemoryPreferences, MemoryQuery, MemoryRecord, SkillRecord, SessionRecord, SessionSearchHit, ReviewState } from "./personal-types";

export type PersonalTab = "memories" | "skills" | "history";
export class PersonalStore {
  preferences: MemoryPreferences = { learningEnabled: true, persistHistory: true, autoSaveUserFacts: true };
  preferencesLoaded = false;
  tab: PersonalTab = "memories";
  memories: MemoryRecord[] = [];
  skills: SkillRecord[] = [];
  sessions: SessionRecord[] = [];
  searchHits: SessionSearchHit[] = [];
  query = "";
  state: ReviewState | null = null;
  busy = false;
  error: string | null = null;
  version = 0;
  dirty = true;
  private generation = 0;
  private prefsGeneration = 0;
  private resumeGeneration = 0;
  constructor(private changed: () => void = () => State.notify()) {}
  notify() { this.version++; this.changed(); }
  invalidate() { this.dirty = true; this.notify(); }
  async loadPreferences() {
    const generation = ++this.prefsGeneration;
    try {
      const preferences = await Bridge.memoryPreferences();
      if (generation === this.prefsGeneration) { this.preferences = preferences; this.preferencesLoaded = true; this.error = null; }
    } catch (error) { if (generation === this.prefsGeneration) this.error = chatDiagnostic(error); }
    this.notify();
  }
  async setPreferences(preferences: MemoryPreferences) {
    // Invalidate a boot-time read before the explicit user's mutation starts.
    const generation = ++this.prefsGeneration;
    try {
      await Bridge.focusWindow(true);
      const saved = await Bridge.memorySetPreferences(preferences);
      if (generation === this.prefsGeneration) { this.preferences = saved; this.preferencesLoaded = true; this.error = null; }
    } catch (error) { if (generation === this.prefsGeneration) this.error = chatDiagnostic(error); }
    this.notify();
  }
  async refresh() {
    const generation = ++this.generation;
    const tab = this.tab;
    const query: MemoryQuery = { query: this.query.trim(), scope: null, state: this.state, limit: 100 };
    this.dirty = false; this.busy = true; this.error = null; this.notify();
    try {
      if (tab === "memories") {
        const records = await Bridge.memoryList(query);
        if (generation === this.generation) this.memories = records;
      } else if (tab === "skills") {
        const records = await Bridge.skillsList(query);
        if (generation === this.generation) this.skills = records;
      } else if (query.query) {
        const hits = await Bridge.historySearch(query.query);
        if (generation === this.generation) { this.searchHits = hits; this.sessions = [...new Map(hits.map((hit) => [hit.session.conversationId, hit.session])).values()]; }
      } else {
        const sessions = await Bridge.historyList();
        if (generation === this.generation) { this.sessions = sessions; this.searchHits = []; }
      }
    } catch (error) { if (generation === this.generation) this.error = chatDiagnostic(error); }
    finally { if (generation === this.generation) { this.busy = false; this.notify(); } }
  }
  async resume(session: SessionRecord) {
    const generation = ++this.resumeGeneration;
    const selection = AgentChat.selectionGeneration, navigation = State.navigationGeneration;
    const messages = await Bridge.historyMessages(session.conversationId);
    if (generation !== this.resumeGeneration || selection !== AgentChat.selectionGeneration || navigation !== State.navigationGeneration || State.paused) return null;
    const conversation = AgentChat.restoreHistory(session, messages);
    Attachments.ensureLoaded(conversation.id);
    return conversation;
  }
  async forgetConversation(conversationId: string) {
    await Bridge.focusWindow(true);
    const conversation = AgentChat.conversations.find((entry) => entry.id === conversationId);
    if (conversation?.runId && !await cancelAgentChat(conversationId, conversation.runId)) return false;
    // Acknowledgement can precede termination; don't delete a still-owned turn.
    if (conversation?.runId) return false;
    await Bridge.permissionsRevoke(conversationId);
    await Bridge.historyForget(conversationId);
    Attachments.forget(conversationId); AgentChat.forgetLocal(conversationId);
    await this.refresh();
    return true;
  }
}
export const Personal = new PersonalStore();
observeAgentChatEvents((event) => { if (event.kind === "completed" || event.kind === "learning") Personal.invalidate(); });
