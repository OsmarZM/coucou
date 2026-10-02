// Fixture sintética isolada: nenhum processo, modelo, arquivo real ou chat externo.
import "../src/style.css";
import { Bridge } from "../src/core/bridge";
import { AgentChat, handleAgentChatEvent } from "../src/core/agent-chat";
import { State } from "../src/core/state";
import { Island } from "../src/island/island";
import type { Attachment } from "../src/core/document-types";
import type { MemoryRecord, SkillRecord } from "../src/core/personal-types";

AgentChat.conversations.splice(0); AgentChat.selectedId = null;
AgentChat.statuses = ["codex", "claude", "gemini", "copilot"].map((agent) => ({ agent: agent as "codex" | "claude" | "gemini" | "copilot", available: true, path: null, detail: "Prévia sintética: nenhuma chamada a CLI ou modelo.", readonlyOnly: agent === "claude", personalSupported: agent === "codex" || agent === "claude" }));
Bridge.agentChatStatus = async () => AgentChat.statuses;
Bridge.focusWindow = async () => null;
const prefs = { learningEnabled: true, persistHistory: true, autoSaveUserFacts: true };
Bridge.memoryPreferences = async () => prefs;
Bridge.memorySetPreferences = async (next) => Object.assign(prefs, next);
const source = { kind: "inference" as const, reference: "conversa:fixture", evidence: "O usuário pediu manter o visual escuro e compacto." };
const memories: MemoryRecord[] = [{ id: "memory-fixture", kind: "preference", key: "Visual do Coucou", content: "Preferir o card escuro, compacto e sem deslocar o Mochi.", scope: "user", source, confidence: .9, state: "approved", revision: 1, replacesId: null, baseRevision: null, createdAt: 1, updatedAt: 1 }];
const skills: SkillRecord[] = [{ id: "skill-fixture", name: "Revisar documento", description: "Leitura com cobertura explícita", body: "1. Ler os anexos selecionados.\n2. Informar as páginas e trechos processados.\n3. Pedir autorização antes de enviar conteúdo para outra conversa.", scope: "conversation:fixture", source, ownership: "coucou", state: "pending", revision: 1, version: 1, replacesId: null, baseRevision: null, createdAt: 1, updatedAt: 1 }];
Bridge.memoryList = async () => memories;
Bridge.skillsList = async () => skills;
Bridge.skillsDiff = async (id, revision) => ({ candidateId: id, revision, predecessorId: null, baseRevision: null, ownership: "coucou", before: "", after: skills[0].body, diff: `+ ${skills[0].body.replaceAll("\n", "\n+ ")}` });
Bridge.memoryApprove = async () => Object.assign(memories[0], { state: "approved" as const, revision: 2 });
Bridge.skillsApprove = async () => Object.assign(skills[0], { state: "approved" as const, revision: 2 });
Bridge.historyList = async () => [{ conversationId: "saved-fixture", agent: "codex", sessionId: "01999999-3333-7333-8333-333333333333", cwd: null, title: "Conversa pessoal salva", messageCount: 2, updatedAt: 1 }];
Bridge.historyMessages = async (conversationId) => [{ id: "saved-user", conversationId, role: "user", content: "Meu documento foi lido por inteiro?", createdAt: 1 }, { id: "saved-assistant", conversationId, role: "assistant", content: "A prévia registra 3 de 5 páginas. Esta é uma resposta sintética.", createdAt: 1 }];
Bridge.historyContext = async (contextId, _limit, budget = 65536) => ({ contextId, text: "Histórico sintético", messages: AgentChat.timeline().map((message) => ({ ...message, contentTruncated: false })), bytes: 100, budget, truncated: false });
Bridge.permissionsRevoke = async () => {};
const documents = new Map<string, Attachment[]>();
Bridge.documentsList = async (id) => documents.get(id) ?? [];
Bridge.documentsChoose = async () => ["fixture:relatorio.pdf", "fixture:notas.txt"];
Bridge.documentsIngest = async (conversationId, paths) => {
  const added = paths.map((path, index) => ({ id: crypto.randomUUID(), conversationId, name: path.split(":")[1], kind: index === 0 ? "pdf" as const : "text" as const, size: 12000, sha256: "fixture", createdAt: 1, status: "ready" as const, message: null, coverage: { extractedChars: 1600, totalPages: index === 0 ? 5 : null, readPages: index === 0 ? 3 : null, emptyPages: [], truncated: index === 0, notes: ["Amostra sintética"] } }));
  documents.set(conversationId, [...(documents.get(conversationId) ?? []), ...added]); return added;
};
Bridge.documentsRemove = async (id, attachmentId) => { documents.set(id, (documents.get(id) ?? []).filter((item) => item.id !== attachmentId)); };
Bridge.chatSend = async () => ({ text: "Resposta simulada da API; nenhum modelo foi chamado." });
const personal = AgentChat.create({ agent: "codex", cwd: "", writable: false });
const previous = AgentChat.begin(personal.id, "Posso conversar sem escolher uma pasta de projeto?");
handleAgentChatEvent({ ...previous, kind: "session", data: { sessionId: "01999999-1111-7111-8111-111111111111" } });
handleAgentChatEvent({ ...previous, kind: "message", data: { text: "Sim. Esta prévia demonstra o contexto pessoal contínuo e anexos compartilhados. Nenhum CLI ou modelo foi chamado." } });
handleAgentChatEvent({ ...previous, kind: "completed", data: { status: "completed" } });
Bridge.agentChatStart = async (request) => {
  handleAgentChatEvent({ ...request, kind: "session", data: { sessionId: request.sessionId ?? "01999999-2222-7222-8222-222222222222" } });
  if (request.query.toLowerCase().includes("enviar")) {
    handleAgentChatEvent({ ...request, kind: "approval", data: { requestId: "external-fixture", title: "Enviar mensagem para outro chat", detail: `Destino: chat-fixture-123\nMensagem integral:\n${request.query}\n\nApenas esta mensagem. A fixture não envia nada.`, choices: ["allow", "deny"] } }); return;
  }
  handleAgentChatEvent({ ...request, kind: "delta", data: { text: "Resposta em streaming simulada. " } });
  handleAgentChatEvent({ ...request, kind: "activity", data: { processCount: 1 } });
  const capturedAt = Date.now() / 1000;
  handleAgentChatEvent({ ...request, kind: "usage", data: { provider: request.agent, sessionId: request.sessionId, runId: request.runId, sourceVersion: "fixture", capturedAt, tokens: { scope: "conversation", source: "fixture", quality: "observed", capturedAt, unit: "tokens", input: 120, output: 30, cachedInput: null, cacheCreation: null, reasoning: null, total: 150 }, lastTurnTokens: null, context: null, quota: { availability: "unavailable", accountScope: null, source: "fixture", capturedAt, windows: [], message: "Cota não disponível na fixture." } } });
  window.setTimeout(() => { handleAgentChatEvent({ ...request, kind: "delta", data: { text: "Anexos, processos e permissões são apenas dados de teste." } }); handleAgentChatEvent({ ...request, kind: "completed", data: { status: "completed" } }); }, 400);
};
Bridge.agentChatDecide = async (conversationId, runId, requestId, decision) => {
  handleAgentChatEvent({ conversationId, runId, kind: "approvalResolved", data: { requestId } });
  handleAgentChatEvent({ conversationId, runId, kind: "message", data: { text: `Decisão simulada: ${decision}. Nenhuma mensagem externa foi enviada.` } });
  handleAgentChatEvent({ conversationId, runId, kind: "completed", data: { status: "completed" } }); return true;
};
Bridge.agentChatCancel = async (conversationId, runId) => { handleAgentChatEvent({ conversationId, runId, kind: "completed", data: { status: "interrupted" } }); return true; };
State.settings.soundEnabled = false; State.settings.userPinned = true; State.loadIntegrationTasks();
const island = new Island(document.getElementById("root")!); island.alert("prompt");
