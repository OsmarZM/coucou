import { Bridge } from "../personal/core/bridge";
import { clear, h } from "../views/dom";

export interface ManagedModelSettings {
  setupDecision: "pending" | "declined" | "accepted";
  enabled: boolean;
  agentName: string;
  autostart: boolean;
  approvedProviders: string[];
  model: { platform: string; modelId: string } | null;
}
export interface ManagedModelStatus {
  settings: ManagedModelSettings;
  runtimeInstalled: boolean;
  running: boolean;
  managedByOtherInstance: boolean;
  port: number | null;
  processId: number | null;
  availableProviders: string[];
  revision: string;
  nodeVersion: string;
  lastError: string | null;
}

export const FREE_PROVIDERS = ["groq", "cerebras"] as const;

export function managedConfiguration(
  current: ManagedModelSettings, name: string, provider: string, modelId: string,
  autostart: boolean, enabled = current.enabled,
): ManagedModelSettings {
  if (!FREE_PROVIDERS.some(value => value === provider)) throw new Error("Escolha um provedor disponível.");
  const trimmedName = name.trim(), trimmedModel = modelId.trim();
  if (!trimmedName || [...trimmedName].length > 60 || /[\u0000-\u001f\u007f]/.test(trimmedName)) throw new Error("Informe um nome de até 60 caracteres.");
  if (trimmedModel && (trimmedModel.startsWith("auto") || !/^[A-Za-z0-9._/-]{1,160}$/.test(trimmedModel))) throw new Error("Informe o identificador exato do modelo; seleção automática não está habilitada.");
  if (enabled && !trimmedModel) throw new Error("Informe um modelo antes de habilitar.");
  if (autostart && !enabled) throw new Error("Habilite o serviço antes de iniciar automaticamente.");
  return { setupDecision: "accepted", enabled, agentName: trimmedName, autostart, approvedProviders: [provider], model: trimmedModel ? { platform: provider, modelId: trimmedModel } : null };
}

export async function buildManagedModelsSection(): Promise<HTMLElement> {
  const body = h("div", { style: "display:flex;flex-direction:column;gap:12px" });
  const section = h("section", {}, h("h2", { text: "FreeLLMAPI · serviço opcional com cotas dos provedores" }), body);
  let status: ManagedModelStatus | null = null;
  let busy = false;
  let feedback = "";
  let failed = false;

  async function run(label: string, operation: () => Promise<ManagedModelStatus | null>) {
    if (busy) return;
    busy = true; feedback = label; failed = false; draw();
    try {
      const next = await operation();
      if (!next) throw new Error("Abra as configurações no aplicativo para configurar o serviço local.");
      status = next; feedback = "Operação concluída.";
    } catch (error) {
      failed = true; feedback = String(error).replace(/^Error:\s*/, "");
    } finally { busy = false; draw(); }
  }

  function action(label: string, task: () => Promise<ManagedModelStatus | null>, progress = label) {
    const button = h("button", { text: label, onclick: () => { void run(progress, task); } });
    button.disabled = busy;
    return button;
  }

  function draw() {
    clear(body);
    body.append(h("p", { class: "hint", text: "O agente pessoal Coucou usa qualquer provedor configurado em Provedores do chat, sem instalar este serviço. Hermes/FreeLLMAPI é uma opção adicional. O FreeLLMAPI usa as contas e cotas dos provedores que você autorizar. Não existe um saldo garantido de 7 bilhões de tokens. A conta do provedor determina a gratuidade; confirme seu plano e seus limites. O supervisor e as CLIs delegadas têm consumos separados." }));
    if (feedback) body.append(h("div", { class: failed ? "notice err" : "notice", role: busy ? "status" : "alert", "aria-live": "polite", text: feedback }));
    if (!status) {
      body.append(h("div", { class: "notice warn", text: "Este serviço é opcional e começa desabilitado. Nenhum modelo é consultado ao abrir esta tela." }), action("Consultar estado", () => Bridge.managedServiceStatus()));
      return;
    }
    const settings = status.settings;
    if (settings.setupDecision !== "accepted") {
      body.append(h("p", { text: "Gostaria de usar APIs com cotas gratuitas?" }), h("p", { class: "hint", text: "Você precisará configurar uma chave. Ao enviar uma pergunta, mensagens e documentos selecionados serão processados pelo provedor habilitado." }));
      body.append(h("div", { class: "row" },
        action("Configurar agora", () => Bridge.managedServiceConfigure({ ...settings, setupDecision: "accepted", enabled: false, autostart: false })),
        action("Continuar sem ativar", () => Bridge.managedServiceConfigure({ ...settings, setupDecision: "declined", enabled: false, autostart: false })),
      ));
      if (settings.setupDecision === "declined") body.append(h("div", { class: "hint", text: "Sua escolha foi salva. As conversas existentes continuam disponíveis." }));
      return;
    }
    const name = h("input", { type: "text", value: settings.agentName, maxlength: "60", "aria-label": "Nome de exibição do Hermes pessoal", style: "flex:1;min-width:160px" });
    const provider = h("select", { "aria-label": "Provedor autorizado" }, h("option", { value: "groq", text: "Groq" }), h("option", { value: "cerebras", text: "Cerebras" }));
    provider.value = settings.model?.platform ?? settings.approvedProviders[0] ?? "groq";
    const model = h("input", { type: "text", value: settings.model?.modelId ?? "", placeholder: "Identificador exato do modelo na sua conta", "aria-label": "Modelo do agente", style: "flex:1;min-width:180px", autocomplete: "off", spellcheck: "false" });
    const startup = h("input", { type: "checkbox", "aria-label": "Iniciar serviço ao entrar no Windows" }); startup.checked = settings.autostart;
    startup.disabled = busy || !settings.enabled || !status.runtimeInstalled;
    for (const field of [name, provider, model]) field.disabled = busy;
    const desired = (enabled = settings.enabled) => managedConfiguration(settings, name.value, provider.value, model.value, startup.checked, enabled);
    body.append(h("div", { class: "row" }, h("label", { text: "Nome de exibição do Hermes" }), name));
    body.append(h("div", { class: "row" }, h("label", { text: "Provedor e modelo" }), provider, model));
    body.append(h("label", { class: "row" }, startup, h("span", { text: "Iniciar apenas este serviço ao entrar no Windows" })));
    body.append(h("div", { class: "hint", text: "Essa preferência é independente de iniciar o Coucou. O login inicia infraestrutura, sem perguntas, tarefas ou retomadas automáticas. Fallback para outro provedor está desligado." }));
    body.append(h("div", { class: "row" }, action(status.running ? "Salvar configuração e parar serviço" : "Salvar configuração", () => Bridge.managedServiceConfigure(desired()))));
    body.append(h("div", { class: "hint", text: `Runtime: ${status.runtimeInstalled ? "instalado" : "não instalado"}. Serviço: ${status.running ? "em execução" : "parado"}. ${status.managedByOtherInstance ? "Gerenciado por outra abertura do Coucou." : ""}` }));

    const key = h("input", { type: "password", placeholder: status.availableProviders.includes(provider.value) ? "Chave guardada; deixe vazio para manter" : "Chave do provedor selecionado", "aria-label": "Chave do provedor", autocomplete: "off", spellcheck: "false", style: "flex:1;min-width:180px" });
    key.disabled = busy || status.running;
    const saveKey = action("Guardar chave", async () => {
      const value = key.value.trim(); key.value = "";
      if (!value) throw new Error("Informe uma chave para guardar.");
      return Bridge.managedServiceSetProviderKey(provider.value, value);
    }); saveKey.disabled = busy || status.running;
    const removeKey = action("Remover chave", () => Bridge.managedServiceSetProviderKey(provider.value, "")); removeKey.disabled = busy || status.running;
    body.append(h("div", { class: "row" }, h("label", { text: "Credencial própria" }), key, saveKey, removeKey));
    body.append(h("div", { class: "hint", text: "A chave é guardada no cofre do Windows. Logins e chaves globais das CLIs não são importados. Pare o serviço antes de trocar credenciais." }));
    const provision = action("Instalar runtime fixado", () => Bridge.managedServiceProvision(), "Baixando, compilando e verificando Node e FreeLLMAPI fixados. A primeira instalação pode demorar em disco lento; prazo máximo de 60 minutos. Nenhum modelo será consultado.");
    provision.disabled = busy || status.runtimeInstalled;
    // Disabling uses the last saved valid configuration. Unsaved form errors or
    // a checked autostart option must never prevent stopping this optional mode.
    const enable = action(settings.enabled ? "Desabilitar modo gratuito" : "Habilitar modo gratuito", () => Bridge.managedServiceConfigure(settings.enabled ? { ...settings, enabled: false, autostart: false } : desired(true)));
    enable.disabled = busy || (!settings.enabled && (!status.runtimeInstalled || !status.availableProviders.includes(provider.value)));
    const start = action("Iniciar serviço", () => Bridge.managedServiceStart(), "Verificando todos os arquivos e iniciando o serviço local. A verificação pode levar vários minutos em disco lento; nenhum modelo será consultado."); start.disabled = busy || !settings.enabled || status.running || !status.runtimeInstalled;
    const stop = action("Parar serviço", () => Bridge.managedServiceStop()); stop.disabled = busy || !status.running;
    body.append(h("div", { class: "row" }, provision, enable, start, stop, action("Atualizar estado", () => Bridge.managedServiceStatus())));
    body.append(h("div", { class: "hint", text: "Instalar e iniciar o serviço confirma apenas a infraestrutura. Compatibilidade do modelo com ferramentas e uso real dependem de uma conversa autorizada." }));
    if (status.lastError) body.append(h("div", { class: "notice err", text: status.lastError }));
  }
  try { status = await Bridge.managedServiceStatus(); }
  catch (error) { failed = true; feedback = String(error).replace(/^Error:\s*/, ""); }
  draw();
  return section;
}
