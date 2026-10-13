import type { ChatAgent } from "./bridge";

export type ReviewState = "pending" | "approved" | "rejected" | "superseded";
export type SourceKind = "user" | "inference" | "document" | "import";
export interface MemoryPreferences { learningEnabled: boolean; persistHistory: boolean; autoSaveUserFacts: boolean }
export interface SourceRef { kind: SourceKind; reference: string; evidence: string }
export interface MemoryQuery { query: string; scope: string | null; state: ReviewState | null; limit: number }
export interface MemoryCandidate { id?: string | null; expectedRevision?: number | null; kind: "preference" | "fact" | "procedure"; key: string; content: string; scope: string; source: SourceRef; confidence: number }
export interface MemoryRecord extends Omit<MemoryCandidate, "id" | "expectedRevision"> { id: string; state: ReviewState; revision: number; replacesId: string | null; baseRevision: number | null; createdAt: number; updatedAt: number }
export interface SkillCandidate { id?: string | null; expectedRevision?: number | null; name: string; description: string; body: string; scope: string; source: SourceRef; ownership: "user" | "coucou" | "imported" }
export interface SkillRecord extends Omit<SkillCandidate, "id" | "expectedRevision"> { id: string; state: ReviewState; revision: number; version: number; replacesId: string | null; baseRevision: number | null; createdAt: number; updatedAt: number }
export interface SkillDiff { candidateId: string; revision: number; predecessorId: string | null; baseRevision: number | null; ownership: string; before: string; after: string; diff: string }
export interface SkillExport { directoryName: string; filename: string; content: string; version: number; ownership: string }
export interface SessionRecord { conversationId: string; agent: ChatAgent | string; sessionId: string | null; cwd: string | null; contextId?: string | null; title: string; messageCount: number; updatedAt: number }
export interface StoredMessage { id: string; conversationId: string; role: "user" | "assistant" | "tool"; content: string; createdAt: number }
export interface SessionSearchHit { session: SessionRecord; messageId: string; excerpt: string }
export interface PersonalContextMessage { id: string; conversationId: string; agent: ChatAgent; role: "user" | "assistant" | "tool"; content: string; createdAt: number; contentTruncated: boolean }
export interface PersonalContextHistory { contextId: string; text: string; messages: PersonalContextMessage[]; bytes: number; budget: number; truncated: boolean }
export interface ContextReference { id: string; revision: number; scope: string; source: SourceKind }

export interface TokenUsage { scope: "conversation" | "turn" | "model_call"; source: string; quality: "observed" | "partial"; capturedAt: number; unit: "tokens"; input: number | null; output: number | null; cachedInput: number | null; cacheCreation: number | null; reasoning: number | null; total: number | null }
export interface ContextUsage { source: string; quality: "observed" | "partial"; capturedAt: number; unit: "tokens"; used: number | null; capacity: number | null }
export interface RateWindow { id: string; source: string; capturedAt: number; quality: "observed" | "partial"; unit: "percent"; usedPercent: number | null; remainingPercent: number | null; durationMinutes: number | null; resetsAt: number | null; state: "available" | "awaiting_update" | "unavailable" }
export interface AgentUsage { provider: string; sessionId: string | null; runId: string; sourceVersion: string | null; capturedAt: number; tokens: TokenUsage | null; lastTurnTokens: TokenUsage | null; context: ContextUsage | null; quota: { availability: "observed" | "unavailable"; accountScope: string | null; source: string; capturedAt: number; windows: RateWindow[]; message: string | null } }
export interface AgentActivity { provider: string; sessionId: string | null; runId: string; kind: "tool" | "subagent"; id: string; status: "running" | "pending" | "completed" | "failed" | "unknown"; source: string; capturedAt: number; quality: "observed" }
