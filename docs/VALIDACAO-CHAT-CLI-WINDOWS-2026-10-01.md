# Validação do chat CLI do Coucou no Windows

> Registro histórico de 01/10/2026. O estado de entrega 0.1.2 e a decisão posterior de bloquear o chat Gemini/Copilot por isolamento não qualificado estão na [validação de 02/10/2026](VALIDACAO-AGENTE-PESSOAL-WINDOWS-2026-10-02.md).

## Objetivo e escopo

Registrar a implementação autorizada do chat usando as CLIs em `D:\coucou`, em 01/10/2026. Branch local `codex/multiagent-windows`, base `5ae7bd9`; mudanças no working tree, sem commit, push ou publicação.

O chat inicia processos locais para Codex, Claude, Gemini e Copilot. Inclui conversas independentes, pasta do projeto, consulta padrão, alterações opt-in, streaming, atividade, retomada por ID, Stop e decisões de permissão de uso único. Anthropic API continua por seleção manual. Claude está limitado a leitura. Não foram instaladas CLIs, criadas chaves nem aplicados hooks globais. O `package-lock.json` e os executáveis previamente existentes na raiz foram preservados.

O manual de operação, campos, regras e exceções está em [CHAT-CLI-WINDOWS.md](CHAT-CLI-WINDOWS.md).

## Evidências automatizadas

| Verificação | Resultado |
|---|---|
| `cargo check --workspace --locked` | Aprovado durante a integração; o check final inclui Clippy. |
| `cargo test --workspace --locked` | 68 testes aprovados: backend 47, protocolo 12, relay 6 e processos do relay 3. Dois probes reais são ignorados por padrão e foram executados separadamente. |
| `npm test` | 43 testes aprovados, dos quais 15 cobrem chat CLI. |
| `npx tsc --noEmit` | Aprovado. |
| `npm run build` | Aprovado; duas entradas de produção, sem as fixtures de desenvolvimento. |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Aprovado. |
| `rustfmt --check` dos novos arquivos `agent_chat` e `git diff --check` | Aprovados. A formatação global possui diferenças preexistentes fora deste módulo; não houve reformatação ampla. |
| `npm run pack` | Aprovado: build release, bundle NSIS x64 e cópias versionada/rolling concluídos. |

Os testes de subprocesso usam Node com scripts fictícios, sem CLIs ou modelos. Eles exercitam JSONL, fila FIFO, envelope Codex/ACP, limites de frames, cancelamento de permissão e Job Windows. O teste do Job observa por handles a morte de um processo pai e do filho depois de shutdown. Isso prova containment nesse cenário sintético; não prova a execução de ferramentas por um agente real.

Outros testes cobrem streaming sem duplicação, isolamento de sessão/turno, política efetiva Codex, callbacks desconhecidos, pedidos incompletos/sensíveis, opções de permissão permanente recusadas, restrições de consulta, expiração, decisões duplicadas e navegação durante aquisição de foco.

## Testes com CLIs instaladas

| CLI e cenário | Resultado confirmado | Limite |
|---|---|---|
| Codex CLI 0.159.2: conversa real de consulta | Aprovado com login ChatGPT e provider efetivo `openai`, sandbox `readOnly`, streaming e dois turnos completos. | Diretório temporário vazio; nenhuma ferramenta nem aprovação. |
| Codex CLI 0.159.2: retomada | Aprovado em outro subprocesso pelo mesmo ID nativo; o segundo prompt pediu o marcador da resposta anterior sem repetir esse marcador. | Retomada textual; não qualifica edição de projeto, permissão ou cancelamento real de ferramenta. |
| Claude Code 2.1.87: inicialização real | Handshake de controle aprovado com os argumentos consultivos da implementação. Nenhum frame `user` ou prompt enviado. | Não prova login, acesso ao modelo, streaming autenticado ou aprovação. A CLI pode realizar verificações próprias de startup/autenticação. |
| Gemini CLI e GitHub Copilot CLI | Adapters e testes sintéticos implementados. | Executáveis ausentes do PATH; sessões autenticadas não testadas. |
| Janela Tauri, foco e click-through | Comandos e validações implementados; build Windows. | Smoke manual da janela nativa e instalação/upgrade ainda não executados. |

O smoke Codex durou 30,59 segundos e retornou somente o marcador neutro `COUCOU_CODEX_CHAT_OK`. O relatório do teste confirmou `turns=2`, `nativeResume=verified`, `status=completed`, 16 eventos delta, dois eventos de sessão, zero ferramentas e zero aprovações. A pasta permaneceu vazia. O teste exige ChatGPT e OpenAI antes de iniciar qualquer turno; recusa chave de API/outro provider. Credenciais e dados de conta não foram impressos nem copiados.

O probe Claude durou 2,36 segundos. A prova é somente `initialize`/`control_response success`, com timeout e encerramento do processo. Ambos os probes são `ignored`, protegidos por variáveis de opt-in, e não fazem parte da suíte padrão.

## Inspeção visual sintética

O Playwright exercitou `/dev/chat-preview.html`, que usa a ilha e o chat reais com Bridge simulado. Foram observados pedido completo, negação, desaparecimento do cartão, novo envio, resposta incremental e conclusão. Console sem erros. Nenhum comando de projeto ou modelo foi executado pela fixture.

- `windows/output/playwright/cli-chat-approval.png`: pedido com detalhe e escolhas de uso único.
- `windows/output/playwright/cli-chat-stream.png`: conversa após negação e novo envio.

Isso comprova o comportamento web com mocks. Não substitui a homologação do IPC e da ativação da janela Tauri com clique real no Windows.

## Correções encontradas na validação

- Caminhos canônicos Windows com prefixo `\\?\` causavam erro `EISDIR` no Node 25.6.1 ao abrir scripts npm. Os caminhos já validados agora são convertidos para formato absoluto de drive/UNC antes da execução e do protocolo.
- Argumentos ACP são montados pelo helper do adapter, preservando `plan` do Gemini e a lista consultiva de ferramentas Copilot.
- Inspeção `--help` do Gemini tem Job, limite de saída, cancelamento e timeout de dez segundos.
- A fila de inicialização tem limite agregado de 8 MiB, além da contagem de frames.
- O backend libera o turno antes de emitir conclusão, evitando recusa de um envio imediatamente seguinte.
- A aprovação que abre o painel não toma foco; aquisição de foco acontece no clique explícito de Allow ou entrada de texto.

## Pacote e checks finais

O pacote do chat foi gerado localmente em `windows/release`, versão `0.1.1`, substituindo o build P0 anterior nesse diretório. `npm run pack` concluiu com exit code 0, build release de 6m16s e bundle NSIS x64 de 4,13 MiB. O linker MSVC emitiu apenas a mensagem informativa de criação de biblioteca de importação. Artefatos conferidos em 01/10/2026 após o término às 14:28:37 (UTC-3):

| Artefato | Bytes | SHA-256 |
|---|---:|---|
| `windows/release/Coucou-Windows-0.1.1-setup.exe` | 4.328.997 | `644C59A189BE0B0CF207BBE078B5140D9B7EE82C86C0ADAA2CB2245B1BF51AB4` |
| `windows/release/Coucou-Windows-setup.exe` | 4.328.997 | `644C59A189BE0B0CF207BBE078B5140D9B7EE82C86C0ADAA2CB2245B1BF51AB4` |
| `windows/target/release/coucou.exe` | 7.461.888 | `B6529880BA3BF0360120AF0AE4E4E2491A8961176F619EDACCC748A747A7259B` |
| `windows/target/release/coucou-hook.exe` | 223.232 | `6708A1E31DEE81F414BBF72E7CC73E410622557C229F25518A0FB0C3F55DE24C` |

As duas cópias do instalador têm o mesmo hash e diferem do pacote P0 anterior. A pasta `dist` não contém as fixtures de desenvolvimento. Não houve instalação, upgrade, commit, push ou publicação.

Foi observada uma instância antiga executando `D:\Coucou\coucou.exe`, que foi preservada. Para testar o novo build, sair dessa instância e instalar o pacote ou iniciar `D:\coucou\windows\target\release\coucou.exe`. A proteção de instância única pode fazer a abertura do novo executável apenas ativar a versão já aberta. Encerrar/instalar é uma ação de uso posterior; não foi feito durante a validação.
