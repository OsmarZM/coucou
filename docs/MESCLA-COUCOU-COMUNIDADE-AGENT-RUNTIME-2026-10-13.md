# Coucou 0.4.0 e Agent Runtime: integração local

## Objetivo

Usar a interface e os recursos atuais do Coucou comunitário com o agente pessoal, o supervisor Hermes, FreeLLMAPI e o registro de ferramentas locais. Esta é uma build personalizada, identificada como **0.4.0-agent.1**, não um pacote publicado por Louis-CFM.

## Origem e preservação

- Comunidade: `https://github.com/Louis-CFM/coucou`, `origin/main` em `71b773f08101237e1ec950efdaee17a626c830e6`, consultado em 13/10/2026.
- Release Windows oficial mais recente consultada: `windows-v0.4.0`, publicada em 11/10/2026.
- Branch local de integração: `codex/coucou-official-agent-runtime`.
- Checkout isolado: `C:\Users\FortaTech\.codex\worktrees\coucou-official-agent-runtime\coucou`.
- O checkout original `D:\coucou` e a instalação em `D:\Nova pasta\Coucou` foram preservados durante a integração.
- O snapshot `75a42e2` guarda as alterações locais anteriores. `18fc099` registra a integração inicial com 0.3.0; a mesclagem seguinte incorpora o main comunitário atual e a base Windows 0.4.0.

## Como as funcionalidades convivem

| Área | Responsabilidade |
| --- | --- |
| Chat com provedores, hooks, atalhos, roupas, idiomas, desktop e vínculo com telefone | Implementação oficial atual. |
| Modo **Agente local** no chat | Sessões pessoais e de projeto com os adapters qualificados; Hermes/FreeLLMAPI, histórico, anexos, aprendizado e ferramentas locais. |
| `src/personal/` | Estado e componentes do agente pessoal, isolados dos modelos e sessões do chat oficial. Reexports mantêm os imports anteriores compatíveis. |
| `agent_wiring.rs` | Registra os comandos e serviços pessoais no backend. O despacho oficial continua responsável pelos comandos da comunidade. |
| Configurações Hermes/FreeLLMAPI | Seções adicionais na janela de configurações do Windows; usa o cofre e os runtimes supervisionados anteriores. |
| Permissões | Aprovação explícita por chamada no backend. A apresentação local aguarda cartões nativos existentes e não concede permissão automaticamente. |

Fluxo local: chat → **Agente local** → Hermes/FreeLLMAPI ou CLI qualificada → chamada estruturada → broker Rust → confirmação → ferramenta → resposta estruturada.

O modo local é criado somente quando selecionado e não inicia tarefas por essa seleção. O provedor escolhido no chat oficial, monitor, atalhos, configurações de telefone e suas chaves permanecem independentes. O áudio pessoal reutiliza o motor oficial, respeitando mute, volume e sons customizados.

## Guardas preservadas

- As oito ferramentas locais continuam tipadas e sem comando arbitrário: aberturas fixas, consulta de processos/lixeira, organização somente dry-run e versão de CLI autorizada.
- O Hermes mantém o catálogo fechado de dezesseis ferramentas `mcp__coucou__*`. Ferramentas nativas, terminal livre, configurações herdadas e chamadas auxiliares não autorizadas seguem bloqueados.
- Auxiliares não recebem as novas ações locais. IDs de chamada, sessão e turno, orçamento e revogação continuam no broker.
- A integração pessoal e suas chamadas Win32 são habilitadas sob `cfg(windows)`. A interface usa a disponibilidade informada pelo boot e não habilita o modo local em plataformas sem o backend.
- Campos pessoais de preferência não podem substituir provider/model, URL customizada, monitor ou atalhos oficiais.
- O relay mantém o stdout neutro oficial (incluindo JSON para Copilot), aceita o `--event` das configurações locais anteriores e limita stdin a 1 MiB e 500 ms. Assim uma entrada que não termina não segura a CLI indefinidamente.
- Não foram reescritos hooks globais nem configurações de fornecedores.

## Ajustes necessários ao atualizar o upstream

Os conflitos iniciais envolveram 34 arquivos. A interface oficial foi mantida, e as funcionalidades pessoais foram integradas explicitamente em vez de substituir a tela oficial antiga por uma cópia do fork. A atualização seguinte ao main atual teve quatro conflitos, resolvidos preservando a inicialização de ambos os conjuntos de serviços.

A comunidade acrescentou entradas incompletas e nomes de marcas ao catálogo macOS. O gerador Windows passa a compilar somente entradas com todas as línguas suportadas. A verificação continua exigindo tradução completa de cada chave usada pela interface Windows; nomes de marcas mantêm o fallback original. Não foram inventadas traduções nem desativada a verificação de cobertura.

O empacotamento respeita `CARGO_TARGET_DIR` e sincroniza o relay para o caminho de recursos da cópia isolada. A versão MSI é `0.4.0.1`; a versão do aplicativo e dos arquivos de pacote é `0.4.0-agent.1`.

## Validação

| Verificação | Evidência |
| --- | --- |
| `cargo check --workspace --all-targets` | Aprovado na base 0.4.0 integrada. |
| `cargo test --workspace` | 590 aprovados, zero falhas, cinco opt-in ignorados: aplicativo 545, protocolo 12, relay 30 e testes do executável relay 3. |
| Frontend e TypeScript | 497 aprovados, zero falhas, um opt-in ignorado; TypeScript e Vite aprovados. |
| Política do bootstrap Hermes | 14 aprovados, sem fornecedor remoto. |

Os testes de frontend incluem isolamento de preferências, seleção do modo local sem iniciar tarefas ou trocar provider, fila de apresentação de aprovações, foco após clique explícito e disponibilidade ausente sem erro de renderização. Os testes Rust exercitam autorização simulada, consulta real de processos e integridade do dry-run. Não comprovam clique humano na janela nativa.

NSIS e MSI gerados com sucesso por `npm run pack`. Política Python: 14 testes aprovados. O roteador FreeLLMAPI passou uma qualificação com runtime real e seis chamadas a fornecedores simulados. O probe Hermes passou com Python real e o executável Rust novo: catálogo restrito de dezesseis ferramentas nas quatro requisições de modelo, duas chamadas ao broker e bloqueio das tentativas de terminal/modo não autorizado. Modelo e broker desse probe são fixtures. Inicialização Hermes: 51,068 s; nova sessão: 32,237 s; esse custo ainda merece otimização.

Clippy estrito (`-D warnings`) não passou: onze avisos no código comunitário e seus testes com Rust 1.98 (nomes de enum, iterator, lifetime, divisão de blocos, organização de testes, clones, erro grande de API e assert constante). Não foram ocultados com supressão global. `cargo fmt --all --check` e verificação de whitespace aprovados.

Evidências no checkout isolado: `windows/target/integration-rust-tests.log`, `integration-frontend-final.log`, `integration-clippy.log`, `integration-free-models.log` e `hermes-agent-runtime-0.4-20261013/evidence.json`.

Pacotes versionados: NSIS SHA256 `24502db7c0be137f8a5abaee2796b3eeee57052519287786e7d925c305025507`; MSI SHA256 `ff80314788ad29f7c2d624779eeca8c24cf0be9502924aa4b18ed7cded5d1b72`. O programa em execução, a instalação, a inferência real de fornecedor e os testes Linux são evidências distintas; uma build ou fixture aprovada não comprova essas etapas.

## Reproduzir

Na pasta `windows/` do checkout de integração:

```powershell
npm ci --ignore-scripts
npx tsc --noEmit
npm test
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
node scripts/gen-strings.mjs --check
npm run pack
```

Runtimes gerenciados e credenciais continuam exigindo configuração explícita. Não há instalação automática de CLIs, login de fornecedor ou ativação automática de hooks por este procedimento.

## Entrega e instalação verificadas

Em 13/10/2026 a mescla validada `8c7b98c` foi incorporada ao checkout principal `D:/coucou`, na branch `codex/multiagent-windows`, pelo commit `d08fd7c`. O código anterior permanece na branch `codex/pre-community-merge-20261013`. Foi comprovado que o código original e o snapshot usado na integração eram idênticos; a diferença era somente ignore de bytecode e um cache Python. As regras de ignore locais foram preservadas.

Os instaladores versionados estão em `D:/coucou/windows/release/`. A instalação existente em `D:/Nova pasta/Coucou` foi atualizada para `0.4.0-agent.1` e aberta com sucesso: processo do caminho instalado respondendo, título `Coucou`. O hash das configurações permaneceu idêntico após a instalação. Isso confirma abertura do processo e preservação do arquivo, sem comprovar os cliques de aprovação nem inferência remota.

O executável instalado tem SHA256 `765b8dc62ffa87682e0748b8163a08e29d036430c352759b1ce9de766a8f5f37`. Comparação integral comprovou que ele difere da build usada no probe somente nos três bytes da marca de empacotamento Tauri: `__TAURI_BUNDLE_TYPE_VAR_UNK` → `__TAURI_BUNDLE_TYPE_VAR_NSS`. Nenhum outro byte diverge.

Backup pré-upgrade: `C:\Users\FortaTech\AppData\Local\Coucou-update-backups\before-agent-0.4.0-20261013-142513`. Inclui instalação 0.3.0, configurações roaming e dados locais de personal/documents/inbox/bin. Runtimes gerenciados anteriores foram mantidos. Nenhum push ou publicação comunitária foi efetuado. Builds Linux e chamadas a fornecedores reais permanecem sem qualificação neste computador.
