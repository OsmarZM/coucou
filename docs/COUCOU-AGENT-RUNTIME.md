# Coucou Agent Runtime — fase 1

Data: 09/10/2026. Implementação local em Rust/Tauri 2 e TypeScript. Não é evidência de instalação, publicação ou homologação da interface nativa.

## Objetivo

Adicionar ações locais tipadas ao chat pessoal existente, preservando credenciais, histórico, anexos, hooks e as restrições de cada fornecedor. Esta fase não implementa automação geral da interface gráfica, alteração de configurações, movimentação de arquivos ou exclusão.

## Pontos de integração identificados

| Componente existente | Uso nesta fase |
| --- | --- |
| `windows/src-tauri/src/agent_chat/codex.rs` | Publica ferramentas dinâmicas do broker, recebe `item/tool/call`, confere thread/turno e encaminha resultado ao modelo. Novas chamadas têm limite de 16 por turno e IDs consumidos antes da autorização; repetição do mesmo ID é recusada. |
| `windows/src-tauri/src/hermes_bridge.rs` / `hermes/bootstrap.py` | Publica as mesmas ferramentas por MCP para Hermes/FreeLLMAPI. Confere identidade, token, catálogo efetivo, limite por turno e IDs; encaminha autorização única e resultados pelo broker existente. |
| `windows/src-tauri/src/capabilities.rs` | Acrescenta o catálogo e encaminha chamadas ao runtime. Reutiliza leases de revogação, resolução de Known Folders e handles estáveis contra substituição de pasta durante a aprovação. |
| `windows/src-tauri/src/agent_chat/transport.rs` | Reutiliza autorização única e o evento de cartão existente; não aceita permissão permanente para ações locais. |
| `windows/src/views/chat.ts` / `core/agent-chat.ts` | Mantém o cartão de confirmação com detalhes integrais, foco, turno, pausa e seleção do fornecedor. Mostra a disponibilidade das ações em Avançado. |
| `windows/src-tauri/src/agent_chat/process.rs` | Reutiliza resolução de CLIs oficiais, execução direta sem shell/shims avaliados, Windows Job e encerramento da árvore. |
| `windows/src-tauri/src/lib.rs` / `core/bridge.ts` | Novo comando `agent_runtime_tools` retorna somente metadados; não existe comando IPC genérico para executar ações ou comandos livres. |
| `secrets.rs`, notificações e hooks | Contratos preservados. Não instala hooks, modifica configurações globais, cria credenciais ou transmite mensagens a chats externos. |

O catálogo está disponível no **Codex pessoal direto e no Hermes com o modelo configurado, incluindo FreeLLMAPI, no Windows**. O modelo fornece chamadas estruturadas; a autorização e a execução são sempre do Rust. Claude pessoal mantém ferramentas nativas desativadas e ainda precisa de um canal próprio para ferramentas Coucou. O chat Anthropic API continua com seu fluxo anterior. Hermes expõe agora dezesseis ferramentas MCP do Coucou: as oito anteriores e as oito ações do runtime. Seus shell, ferramentas nativas, plugins e configurações herdadas continuam bloqueados. Auxiliares delegados por Hermes não recebem as novas ações e o despacho também bloqueia uma chamada forjada nesse contexto. Gemini/Copilot mantêm as guardas de isolamento anteriores.

A verificação local encontrou Codex CLI **0.161.0**, enquanto a guarda existente do chat pessoal permite somente **0.159.2**. Essa guarda se aplica ao canal Codex e não impede as ações locais do Hermes/FreeLLMAPI, que não dependem de Codex instalado. A presença das ferramentas no catálogo não comprova disponibilidade do fornecedor remoto. No Codex, ferramentas dinâmicas são registradas ao criar a sessão; sessões anteriores conservam seu catálogo e histórico. No Hermes, a ponte MCP lista o catálogo ao carregar/criar a sessão e a atestação verifica exatamente as dezesseis ferramentas efetivas antes de enviar o prompt.

## FreeLLMAPI e Hermes

Fluxo: chat Coucou → Hermes ACP → modelo configurado via FreeLLMAPI → chamada `mcp__coucou__<ferramenta>` → ponte MCP autenticada → broker Rust → confirmação única → handler local → resultado estruturado → modelo.

As ferramentas usam os mesmos schemas do Tool Registry, com nomes MCP como `mcp__coucou__query_processes` e `mcp__coucou__organize_files_dry_run`. O bootstrap Python, a atestação Rust e o catálogo da ponte foram atualizados em conjunto. A validação mantém a lista fechada; registrar uma ferramenta no MCP não autoriza ferramentas nativas do Hermes. O roteador FreeLLMAPI conserva o provider/model exato e o contrato OpenAI de `tools`, `tool_calls` e resultados `role: tool`, sem fallback.

O provedor/modelo escolhido precisa suportar tool calling. Esta integração não modifica a chave, a seleção de modelo, o roteador instalado ou configurações globais. É necessário executar a build que contém a alteração e recriar/recarregar a sessão Hermes para atualizar o catálogo; uma instância antiga em memória não recebe código novo automaticamente.

## Registro e validação

`windows/src-tauri/src/agent_runtime/mod.rs` contém `ToolRegistry`, `ToolDefinition` e `ToolCall`. O catálogo exporta schemas de objetos com `additionalProperties: false`. A autoridade de execução é a desserialização Rust: enums fechados, argumentos tipados com `deny_unknown_fields`, limites de caminho e validação real do alvo. O schema enviado ao modelo não substitui essas verificações.

Para adicionar uma ferramenta, incluir sua definição, variante tipada, handler e testes. A extensão exige revisão de código; não carrega plugins ou executáveis indicados pelo modelo. A cobertura automatizada confere que todas as definições registradas têm um handler tipado.

| Ferramenta | Parâmetros permitidos | Efeito e limites |
| --- | --- | --- |
| `coucou_open_device_manager` | `{}` | Solicita abertura de `mmc.exe` com o `devmgmt.msc` da pasta de sistema Windows. Não configura dispositivos. |
| `coucou_open_windows_settings` | `page`: `home`, `display`, `bluetooth`, `windows_update` | Abre uma URI `ms-settings:` fixa pelo Explorador. Não permite URI livre nem altera configuração. |
| `coucou_open_application` | `application`: `notepad`, `calculator` | Executáveis fixos da pasta de sistema, sem argumentos do modelo. |
| `coucou_open_directory` | `path` | Pasta local existente ou Downloads/Documentos/Área de Trabalho, validada e mantida estável. Nunca abre um arquivo executável. |
| `coucou_query_processes` | `{}` | Snapshot ToolHelp: até 500 nomes, PIDs e PIDs pais. Não consulta linha de comando, ambiente ou conteúdo da memória dos processos. |
| `coucou_organize_files_dry_run` | `path` | Plano por categoria de até 1000 entradas da pasta, sem recursão; orçamento de dez segundos e 192 KiB para itens retornados. Nenhuma escrita. |
| `coucou_query_recycle_bin` | `{}` | Namespace Shell da lixeira do usuário: até 100 nomes, total e indicação de truncamento. Não restaura, esvazia ou exclui. |
| `coucou_cli_version` | `provider`: `codex`, `claude`, `gemini` | Somente `--version`; timeout de dez segundos. Não aceita prompt, sessão externa, flags ou comandos livres. |

Abrir uma aplicação retorna `launch_requested`, PID e `windowVerified: false`: iniciar o processo não comprova que a janela ficou visível. UAC ou restrições do Windows continuam sob controle do sistema.

A ferramenta da lixeira é um handler Rust com um adapter Shell por **script PowerShell fixo incorporado ao binário**. Executa o PowerShell da pasta de sistema com `-NoProfile -NonInteractive`; não interpola parâmetros do usuário/modelo. O script somente enumera o namespace 10 e serializa nomes. A escolha evita uma nova dependência COM de automação nesta fase e deve ser qualificada na janela nativa antes de considerar a função homologada.

## Fluxo de autorização

1. O modelo do Codex ou Hermes/FreeLLMAPI solicita uma ferramenta estruturada no turno atual.
2. O adapter/ponte confere a identidade do turno, consome o ID e verifica seu orçamento. Hermes usa a ponte local autenticada, vinculada ao contexto ACP.
3. O registro valida nome, tipos e campos. O broker verifica modo pessoal e habilitação do runtime; auxiliares têm essa habilitação desligada.
4. Para pastas, resolve e valida o alvo antes de mostrar o cartão, mantendo os handles de seus ancestrais e da pasta abertos.
5. O cartão existente mostra ferramenta, argumentos, caminho resolvido, dry-run e escopo de uma execução. Oferece somente permitir uma vez ou negar.
6. Somente `allow` permite execução. Cancelamento, pausa/falta de foco, decisão de outro turno, revogação, timeout ou erro não concedem acesso. Não há retry automático.
7. O handler executa a ação limitada e retorna resultado observado. Dados externos não concedem instruções ou autorização. Resultados acima de 256 KiB ou reconhecidos pelo detector de credenciais existente são bloqueados antes do envio ao modelo.

Todas as ferramentas novas pedem confirmação, incluindo consultas. A fase não registra operações destrutivas: implementar movimento, exclusão ou alteração de configurações futuramente exige ferramentas específicas, prévia vinculada ao alvo e nova aprovação.

## Organização em simulação

`agent_runtime/plan.rs` contém o planejamento puro. As categorias são Documentos, Imagens, Videos, Audio, Arquivos e Outros. Arquivos executáveis desconhecidos ficam em Outros no plano; não são executados.

O handler Windows enumera somente a pasta indicada, ignora alvos que não podem ser validados e informa `skipped`, `truncated` e `complete`. Verifica nomes já existentes no destino, colisões entre itens planejados sem diferenciar maiúsculas/minúsculas e categorias ocupadas por arquivo, link ou alvo inacessível. Conflitos são marcados; não escolhe nomes alternativos nem substitui dados. O retorno inclui `dryRun: true` e `writes: 0`.

Exemplo de chamada:

```json
{"name":"coucou_organize_files_dry_run","arguments":{"path":"Downloads"}}
```

Exemplo de resultado de um item:

```json
{"source":"C:\\Users\\Usuario\\Downloads\\relatorio.pdf","destination":"C:\\Users\\Usuario\\Downloads\\Documentos\\relatorio.pdf","conflict":true}
```

Um plano é uma observação, não uma transação reservada para execução futura. Antes de mover arquivos em outra fase será obrigatório recalcular o plano, validar identidades e conflitos e obter aprovação para a alteração concreta.

## Executor CLI e sessões

O executor interno associa uma execução ao canal/turno e gera um identificador único por processo e contador. Captura stdout, stderr, estado de término e código de saída. Cada stream tem limite de 64 KiB; excesso é drenado para evitar bloqueio e o resultado incompleto é recusado. Timeout, cancelamento ou revogação encerram a árvore pelo Windows Job e aguardam a coleta do processo com limite.

Somente handlers Rust constroem programa e argumentos. O modelo escolhe um enum de fornecedor; o resolver existente aceita seu executável oficial ou o entrypoint validado do pacote npm, sem avaliar `.cmd`/`.ps1` como shell. Uma CLI ausente produz erro explícito.

Tarefas reais, autenticação, isolamento e protocolos de conversa permanecem nos adapters existentes. A ferramenta inicial de versão não prova delegação, suporte a permissões ou isolamento de um fornecedor. A futura integração deve usar esses adapters e sua autorização, preservando a regra de não retomar nem enviar mensagens a chats externos sem aprovação específica.

## Plataformas e compatibilidade

As operações Win32 e o executor desta fase estão em `agent_runtime/windows.rs`, compilado somente com `#[cfg(windows)]`. Registro e planejamento usam Rust/Serde portáveis. Fora do Windows, o catálogo público é vazio e o frontend não apresenta disponibilidade de ações. A integração de despacho Windows tem guarda de plataforma.

A build Linux completa não foi executada nesta máquina: não há target Rust Linux instalado e o Ubuntu WSL consultado não expôs Cargo/Rust. O checkout já contém módulos anteriores com imports Windows incondicionais, inclusive `capabilities.rs` e `agent_chat/process.rs`; sua qualificação Linux é uma pendência anterior que não deve ser confundida com aprovação desta fase. Nenhuma dependência, configuração Linux ou workflow foi alterado pelo runtime.

## Validação

Consultar [VALIDACAO-AGENT-RUNTIME-2026-10-09.md](VALIDACAO-AGENT-RUNTIME-2026-10-09.md) para comandos, contagens e limites de evidência.

Execução local, a partir de `windows/`:

```powershell
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
npm test
npx tsc --noEmit
npx vite build
```

Testes de autorização usam decisões simuladas; não comprovam clique no cartão Tauri. Testes de sessão executam scripts PowerShell fixos de fixture sem modelo ou login de fornecedor. Não foi gerado instalador nesta fase; os executáveis existentes na raiz são preservados.
