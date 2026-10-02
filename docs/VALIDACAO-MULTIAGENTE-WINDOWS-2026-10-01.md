# Validação local do Coucou multiagente no Windows

Este relatório registra o snapshot P0 de monitoramento, anterior ao chat CLI. Os hashes abaixo pertencem àquele build; o pacote no mesmo caminho foi posteriormente atualizado. A evidência e os hashes do chat estão em [VALIDACAO-CHAT-CLI-WINDOWS-2026-10-01.md](VALIDACAO-CHAT-CLI-WINDOWS-2026-10-01.md).

## Objetivo e escopo

Registrar as evidências da implementação P0 em `D:\coucou`: monitoramento de Claude Code, Codex CLI, Gemini CLI e GitHub Copilot CLI, sessões independentes, configuração reversível de hooks e preservação do fluxo de aprovação do Claude.

Data: 01/10/2026. Branch: `codex/multiagent-windows`, criada a partir de `5ae7bd9`. A implementação está no working tree, sem commit, push ou publicação. A versão do pacote permanece `0.1.1`; o instalador gerado é um artefato local desta branch.

O `windows/package-lock.json` e os executáveis na raiz (`coucou.exe`, `coucou-hook.exe`, `uninstall.exe`) já existiam como alterações locais antes desta implementação e foram preservados. Os novos binários são gerados em `windows/target` e `windows/release`.

## Ambiente observado

| Item | Evidência nesta execução |
|---|---|
| Sistema/target | Windows, `x86_64-pc-windows-msvc` |
| Node | `v25.6.1` |
| Cargo / Rust | `1.98.1` |
| Codex CLI | `codex-cli 0.159.2`, disponível no PATH |
| Claude Code | `2.1.87`, disponível no PATH |
| Gemini CLI | Ausente do PATH usado nesta execução |
| GitHub Copilot CLI | Ausente do PATH usado nesta execução |

Consultar a versão e o PATH não comprova autenticação nem emissão de eventos. Nenhuma sessão autenticada desses agentes foi conduzida, e nenhuma configuração real de hooks foi aplicada durante o desenvolvimento.

## Resultado dos checks

Comandos executados em `D:\coucou\windows`:

| Verificação | Resultado final |
|---|---|
| `npm run build` | Aprovado: TypeScript e bundle Vite, 36 módulos; também executado no empacotamento |
| `npm test` | Aprovado: 28 testes, zero falhas |
| `cargo test --locked --workspace` | Aprovado: 41 testes, zero falhas (20 backend, 12 protocolo, 6 relay e 3 testes de processo) |
| `cargo clippy --locked --workspace --all-targets -- -D warnings` | Aprovado, exit code 0 na rodada final sequencial |
| `rustfmt --check` nos módulos de protocolo/relay/instalador/transportes alterados | Aprovado |
| `git diff --check` | Aprovado; avisos de conversão LF/CRLF do Git sem erro de whitespace |
| `npm run pack` | Aprovado: release otimizada + NSIS x64, exit code 0; rodada final em 4m36s após a correção de correlação por turno |

`cargo fmt --all --check` ainda aponta formatação preexistente em outros módulos. Foi reproduzido com o conteúdo de `claude.rs` no commit-base `5ae7bd9`; não foi feita uma reformatação geral do projeto. O check de formatação focado passou para `agent-protocol/src/lib.rs`, `hook/src/main.rs`, `hook/src/win.rs`, `hook/tests/neutral.rs`, `src-tauri/src/approvals.rs`, `hooks.rs`, `hooks/installer.rs`, `lib.rs` e `pipe.rs`. Em `island.rs`, duas correções pequenas de lint preservam a formatação anterior.

Uma tentativa de Clippy enquanto havia outra compilação/execução de testes terminou com `os error 32` no build script Tauri (arquivo em uso no Windows). A rodada final foi sequencial e passou. Os builds Rust emitiram um aviso informativo de stdout do linker MSVC ao criar a biblioteca de importação; testes e pacote concluíram normalmente.

## Artefatos finais

Arquivos conferidos após o término do empacotamento, em 01/10/2026 às 11:17:15 no horário local:

| Artefato | Bytes | SHA-256 |
|---|---:|---|
| `windows/release/Coucou-Windows-0.1.1-setup.exe` | 4.251.862 | `55C251CA6AE4A8997474F77F5A3707C1889944682EE008814D36CF623EE1996F` |
| `windows/release/Coucou-Windows-setup.exe` | 4.251.862 | `55C251CA6AE4A8997474F77F5A3707C1889944682EE008814D36CF623EE1996F` |
| `windows/target/release/coucou.exe` | 7.248.384 | `6B03781578F70E5B5E786BC3C19918AC0442C53C3D023C19901C7FB3972A6063` |
| `windows/target/release/coucou-hook.exe` | 223.232 | `6708A1E31DEE81F414BBF72E7CC73E410622557C229F25518A0FB0C3F55DE24C` |

As duas cópias do instalador têm o mesmo hash (4,05 MiB). A conferência dos arquivos não substitui executar o instalador nem homologar a janela nativa e as CLIs.

## Cenários cobertos

- Identidade composta por agente e ID nativo de sessão; duas sessões no mesmo projeto permanecem independentes.
- Seleção de sessão não muda ao receber eventos de outra sessão; eventos de turno anterior são considerados quando há `turnId` conhecido.
- Limites de sessões, passos, resumos e payload; metadados sensíveis, prompts e conteúdo bruto de ferramentas não são repassados à interface.
- Normalização das quatro CLIs e compatibilidade do relay legado Claude; stdout neutro de cada fornecedor, entrada malformada, oversized e stdin que não termina.
- Preservação de handlers externos no mesmo grupo; reinstalação sem reordenar nem criar backup desnecessário; prévia desatualizada, arquivo inválido, backup dos bytes originais e lock concorrente.
- Aprovação com reconhecimento após apresentação; somente clique explícito, pedido ativo e janela da ilha focada podem produzir uma decisão consumida uma vez.
- Ausência de reconhecimento, pedido concorrente, expiração, pausa, troca de view, encerramento de turno/sessão e desconexão devolvem o controle ao fluxo nativo.
- Transição de compacto para o cartão de aprovação; navegação para Settings/upload/confused libera o pedido e não restaura cartão sem pedido válido.
- Aprovação reconhecida no turno B permanece ativa após `Stop` atrasado do turno A e é cancelada por `Stop` de B; falta de IDs mantém o retorno prudente à CLI.
- Named pipe Windows real, com nome isolado de teste: conexão/leitura, rejeição de segunda primeira instância e desconexão de cliente após reconhecimento. Esse cenário não usa a sessão de uma CLI nem o pipe da aplicação em uso.

## Inspeção visual

A fixture `/dev/multiagent-preview.html` usa a ilha real com eventos fictícios. Foram inspecionados os grupos dos quatro agentes junto às quatro integrações de serviços e a troca entre duas sessões Codex do mesmo diretório, uma trabalhando e outra com turno concluído. Também foram inspecionados os quatro cartões de configuração de CLI em `/settings.html`.

Artefatos locais em `D:\coucou\windows\output\playwright`:

- `multiagent.png`: ilha com sessões sintéticas.
- `island-multiagent.png`: captura direta do componente da ilha na mesma fixture.
- `settings-multiagent.png`: página completa de Settings no navegador, sem backend Tauri.

A fixture não é uma entrada do bundle de produção. No navegador, Settings informa que a configuração precisa ser feita dentro do Coucou. Essa inspeção comprova apresentação web com dados simulados; não comprova comportamento da janela nativa, foco/click-through do Windows ou uma integração real com uma CLI.

## Limites de homologação e ativação

| Superfície/cenário | Situação |
|---|---|
| Protocolo, relay e instalador em testes locais | Cobertura automatizada descrita acima |
| Pipe Windows isolado | Exercitado em teste nativo |
| Ilha e Settings no navegador | Inspeção visual sintética |
| Claude Code: sessão real, aprovação e retorno à CLI | Não homologado nesta execução |
| Codex CLI: confiança `/hooks` e emissão real | Não homologado nesta execução |
| Gemini/Copilot CLI: sessão e eventos reais | Não homologados; CLIs ausentes do PATH |
| Instalador NSIS: execução/upgrade e aplicação nativa | Pacote gerado; instalação e smoke manual não executados |
| Codex desktop, VS Code e agentes cloud | Fora da entrega P0 |
| Chat com novos provedores | Etapa posterior; o chat atual permanece Anthropic |

Para ativar, instalar o pacote local, abrir Settings no Coucou, revisar a prévia por agente e confirmar a aplicação. Reiniciar a CLI e, no Codex, revisar/confiar nos handlers via `/hooks`. Receber o primeiro evento real é um passo separado de ter o arquivo configurado. Não duplicar os hooks em camadas de usuário/projeto.

## Documentação de uso

O contrato, campos, fluxo, caminhos efetivos e exceções estão em [MULTIAGENTE-WINDOWS.md](MULTIAGENTE-WINDOWS.md). A evolução posterior permanece em [PLANO-MULTIAGENTE-WINDOWS.md](PLANO-MULTIAGENTE-WINDOWS.md).
