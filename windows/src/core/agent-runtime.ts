/** Catalog metadata only: the browser cannot execute a tool or grant permission. */
export interface RuntimeToolSpec {
  type: "function";
  name: string;
  description: string;
  inputSchema: Record<string, unknown>;
}

export function runtimeAvailability(provider: string, personal: boolean, tools: RuntimeToolSpec[]): string | null {
  if (!["codex", "hermes"].includes(provider) || !personal || !tools.length) return null;
  const channel = provider === "hermes" ? "Hermes com o modelo configurado, incluindo FreeLLMAPI" : "novos chats pessoais Codex com CLI compatível";
  return `Ações locais para ${channel}: abrir aplicativos e configurações, consultar processos e lixeira e simular organização de arquivos. Cada ação pede sua confirmação. A simulação não move arquivos.`;
}
