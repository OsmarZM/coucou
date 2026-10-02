# Coucou multiagente no Windows

## Objetivo

Acompanhar Claude Code, Codex CLI, Gemini CLI e GitHub Copilot CLI na ilha do Coucou. Cada agente possui seu grupo; o seletor identifica sessões independentes, inclusive duas sessões no mesmo diretório.

Esta entrega implementa observabilidade das quatro CLIs e preserva aprovação interativa do Claude. Decisões de Codex, Gemini e Copilot continuam em suas interfaces nativas. O chat interno continua usando Anthropic.

## Como usar

1. Gerar/instalar o pacote Windows e abrir o Coucou.
2. Abrir **Settings** e localizar o cartão da CLI desejada.
3. Conferir o caminho efetivo da configuração e a disponibilidade do relay.
4. Selecionar **Install hooks…**, revisar o diff e confirmar **Apply reviewed hooks**.
5. Reiniciar a sessão da CLI. Para Codex, revisar as definições no `/hooks` e conceder confiança na própria CLI.
6. Enviar trabalho na CLI. O primeiro evento recebido fornece evidência de atividade; um hook instalado sozinho não comprova emissão de eventos.
7. Selecionar o agente e, quando houver mais de uma sessão, escolher projeto/identificador no seletor.

Para remover uma integração, usar **Uninstall hooks…** e revisar a remoção. Somente os handlers identificados como pertencentes ao Coucou são retirados. Reinstalar uma configuração já correta não cria outro hook ou backup.

| Agente | Configuração padrão | Override respeitado | Capacidade nesta entrega |
|---|---|---|---|
| Claude Code | `%USERPROFILE%\.claude\settings.json` | `CLAUDE_CONFIG_DIR` | Sessão, ferramentas, turno e aprovação explícita suportada. |
| Codex CLI | `%USERPROFILE%\.codex\hooks.json` | `CODEX_HOME` | Sessão, ferramentas, turno e interrupção. |
| Gemini CLI | `%USERPROFILE%\.gemini\settings.json` | `GEMINI_CLI_HOME`, raiz anterior a `.gemini` | Sessão, ferramentas, turno e aviso de permissão. |
| Copilot CLI | `%USERPROFILE%\.copilot\hooks\coucou.json` | `COPILOT_HOME` | Sessão, ferramentas, término do agente e notificações. |

Codex também pode carregar hooks por projeto/TOML; Copilot também pode carregar hooks em `.github/hooks`. O Coucou administra os arquivos indicados na tabela. Não copiar seus handlers para outra camada: os fornecedores podem executar os hooks cumulativamente. Se o Agent Island também estiver instalado, evitar dois cartões concorrentes controlando a mesma aprovação.

## Funcionamento e contrato

```text
CLI -> coucou-hook --agent <agente> --event <evento>
    -> adapter/allowlist -> named pipe do usuário
    -> validação Rust -> evento Tauri agent-hook
    -> SessionStore -> grupo e seletor na ilha
```

O crate local `windows/agent-protocol` concentra normalização e validação. Seu envelope usa `protocolVersion: 1`, `agent`, `sessionId`, `eventType`, `cwd` e metadados opcionais. Apenas o backend atribui `requestId` aos pedidos interativos.

`sessionId` precisa ser um identificador nativo válido. A chave interna serializa `[agent, sessionId]`; o diretório nunca substitui essa identidade. Eventos sem identificador confiável são ignorados. O relay legado `coucou-hook <Event>` continua sendo interpretado como Claude.

`Stop` encerra um turno; `SessionEnd` encerra a sessão. Uma ferramenta terminada não é automaticamente bem-sucedida. Inatividade não é conclusão. Quando o fornecedor fornece `turnId`, um evento de outro turno não substitui o turno atual. Sem esse ID, gerações locais protegem timers; não há como recuperar ordenação causal que a CLI não forneceu.

O registro mantém no máximo 48 sessões, retenção de 24 horas, 20 passos por sessão e resumos de 160 caracteres. São dados em memória; reiniciar o Coucou não recupera o histórico anterior.

## Aprovações e exceções

- Apenas Claude `PermissionRequest` com alvo completo e inspecionável produz cartão interativo.
- Comando muito longo, multilinha, ausente ou omitido pela política de metadados permanece no fluxo nativo do Claude. A ilha informa necessidade de ação sem oferecer aprovação cega.
- Apenas um cartão pode permanecer pendente. Outro pedido, inclusive de outra sessão, retorna à CLI; um badge não confirma que o cartão foi apresentado.
- O reconhecimento do cartão ocorre após o frame de apresentação. Sem reconhecimento dentro de 800 ms, o fluxo nativo continua.
- O prazo absoluto do pedido no backend é 109 segundos. A interface libera o cartão antes desse limite.
- A decisão exige clique explícito, janela da ilha focada, aplicativo ativo, pedido reconhecido e ainda válido. É consumida uma vez.
- Pausa, navegação, início de turno, encerramento da sessão, desconexão e expiração invalidam o pedido. Fim/falha/interrupção de turno respeitam o `turnId` quando pedido e evento o fornecem; um término atrasado de outro turno não cancela o cartão atual. Sem esses IDs, o controle volta prudentemente à CLI. Nenhum desses casos concede `allow`.
- Uma decisão aceita pelo backend registra o clique; não comprova execução da ferramenta.

O transporte restringe o pipe ao SID do usuário, recusa clientes remotos, limita concorrência/tamanho e verifica o usuário do servidor no relay. Processos da mesma conta continuam no mesmo domínio de confiança local.

## Privacidade e instalação reversível

O normalizador não encaminha prompts completos, transcripts, patches, conteúdo de arquivo, ambiente ou resultado bruto de ferramentas. Resumos vêm de metadados permitidos, com limite de tamanho e remoção de controles ANSI. Strings com padrões de credenciais, URLs autenticáveis/URLs ou assignments são omitidas conservadoramente.

O instalador não autentica nem executa a CLI. Ele valida o arquivo, mostra prévia vinculada a agente/caminho/operação/bytes, preserva handlers de terceiros, cria backup exclusivo dos bytes originais e substitui a configuração no mesmo volume sob lock Windows. Arquivo inválido ou alterado desde a prévia exige nova revisão.

Segredos do chat continuam no Windows Credential Manager. Nenhuma configuração real dos agentes foi aplicada durante a implementação.

## Build e validação

Executar em `D:\coucou\windows`:

```powershell
npm run build
npm test
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets -- -D warnings
npm run pack
```

O runner de testes frontend usa Node e o TypeScript já presente no projeto; requer Node 20.6 ou posterior. O protocolo é um crate local com `serde`/`serde_json`, sem banco ou cliente de IA. A feature `tokio/macros` permite cancelar a espera ao desconectar o pipe.

Para conferir a aparência com dados fictícios, executar `npm run dev` e abrir `/dev/multiagent-preview.html`. A fixture está explicitamente marcada como sintética e não integra os arquivos de entrada do pacote de produção.

O registro da execução, checks, artefatos e limitações está em [VALIDACAO-MULTIAGENTE-WINDOWS-2026-10-01.md](VALIDACAO-MULTIAGENTE-WINDOWS-2026-10-01.md).

## Limites de suporte

Hooks documentados e testes automatizados não equivalem a homologação de uma CLI real. CLI ausente, sem sessão autenticada observada ou sem evento real permanece não testada.

Esta entrega não declara homologação do Codex desktop, de extensões do VS Code ou de agentes cloud. Cada superfície exige validação própria. Chat com OpenAI/Gemini/Copilot, execução de agentes pelo Coucou e controles adicionais de aprovação permanecem nas etapas posteriores do plano.

Referências oficiais: [Claude](https://code.claude.com/docs/en/hooks), [Codex](https://developers.openai.com/en-US/docs/hooks), [Gemini](https://geminicli.com/docs/hooks/reference/), [Copilot](https://docs.github.com/en/copilot/reference/hooks-reference).
