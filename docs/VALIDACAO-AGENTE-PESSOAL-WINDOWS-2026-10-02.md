# Validação do agente pessoal — Windows 0.1.2

- Data: 02/10/2026.
- Repositório: D:\coucou; branch codex/multiagent-windows.
- Entrega: código, testes e README preparados para o commit local autorizado. Sem push ou publicação.
- Instalador 0.1.2 gerado; worker do release final aprovado em dez casos.
- Executáveis preexistentes da raiz preservados e excluídos do commit.

## Objetivo e funcionamento

O fluxo principal usa personagens animados e um contexto pessoal comum, sem seletor de conversa e sem ativação manual de aprendizado. O armazenamento local conserva histórico, origem das memórias e anexos. A preparação de cada turno recupera contexto com orçamento limitado; não há treinamento dos pesos nem chamadas recorrentes de aprendizado.

Os canais e IDs nativos são separados por fornecedor. Trocar de personagem conserva timeline, rascunho e seleção de anexos, sem iniciar modelo ou enviar mensagem. Projetos e sessões externas ficam em Avançado e não entram automaticamente no contexto pessoal. Conhecimento aprendido não concede permissões.

## Matriz funcional

| Cenário | Evidência | Resultado e limite |
|---|---|---|
| Codex pessoal sem pasta escolhida | Dois turnos reais, ambiente vazio depois de cada resposta e retomada com reinício do app-server | Aprovado com CLI 0.159.2; permissão simulada pela fixture |
| Contar/listar/ler sob autorização | Broker por fornecedor/canal/operação/alvo/profundidade, revogação e cancelamento; contagem e negativa reais | Aprovado; clique de permissão Tauri pendente |
| Escape de alvo | Fixtures Windows para junction/reparse, UNC, ADS, traversal, hardlink e substituição de handles | Aprovado nos testes; redirecionamento empresarial/OneDrive não homologado |
| Conversa pessoal contínua | Binding explícito de canais locais, recuperação limitada e deduplicação | Aprovado; histórico externo não é importado |
| Contexto automático | SQLite/FTS, migração v1, evidência original, preferências corrigidas e fatos declarados conservadores | Aprovado; propostas ambíguas/ações não são consolidadas automaticamente |
| Procedimentos | Diff, revisão exata, versionamento, importação/exportação/restauração e carregamento progressivo | Aprovado nos testes; revisão humana permanece |
| Mensagem a chat externo | Proveniência SQLite, destino e mensagem integral, decisão única por canal/run/nonce | Aprovado nos testes; nenhum prompt foi enviado a chat existente do usuário |
| Duplicatas e concorrência | Recibos persistidos, replay, conflito por sessão/canal e exclusão simultânea | Aprovado; não há retentativa automática |
| Documentos selecionados | Hash, cópia local, formato/cobertura, referências, orçamento e validação integral de credenciais antes dos recortes | Aprovado nos testes e em dez casos com worker release; sem clique/drop OLE nativo homologado |
| PDF/DOCX/texto/código/CSV/JSON | Extração local e leitura progressiva, sem macros/links externos/execução | Implementado; imagem/OCR/PDF só digitalizado/DOC legado indisponíveis |
| Visibilidade | FSM visível por padrão, escolha de fixação/ocultação, foco/draft/drop/execução/confirmação | Aprovado nos testes; hover/DPI/multimonitor/OLE nativos pendentes |
| Português e personagens | Catálogo pt-BR e walkthrough Chrome com fixture | Aprovado no frontend |
| Atividade e processos | IDs por fornecedor/sessão/run, árvore Windows Job e contagem após encerramento | Aprovado nos testes; hooks externos têm observabilidade parcial |
| Tokens e cotas | Snapshots sem soma duplicada, identidade da conta, fonte/horário, disponibilidade e reset vencido | Aprovado; cota Codex observada no teste real, ausência não vira zero |
| Claude pessoal | Adapter com ferramentas nativas desativadas e documentos preparados | Implementado; 2.1.87 detectada, sem turno com modelo homologado nesta entrega |
| Gemini e Copilot | Chat bloqueado nos modos pessoal e projeto antes de preparar contexto ou iniciar CLI; adapters preservados | Gemini 0.62.0 detectada com bypass confirmado por hooks/MCP; Copilot ausente e isolamento não qualificado; instalação/monitoramento separados |
| Instalação/upgrade do Windows | Geração do pacote distinta da instalação | Instalação e uso da janela nativa não executados |

## Verificações locais

- Frontend: 103 testes aprovados, zero falhas.
- TypeScript e build Vite: aprovados; 45 módulos transformados.
- Rust: 142 testes aprovados (121 aplicação, 12 protocolo, 6 hook, 3 integração), zero falhas; três testes reais ignorados na suíte padrão. O smoke pessoal foi executado separadamente, conforme a evidência abaixo.
- cargo fmt --all --check: aprovado.
- cargo clippy --workspace --all-targets -- -D warnings: aprovado na rodada final, incluindo a correção de título.
- Regressão de versão/originator Codex: teste focal aprovado após conferir a inicialização isolada sem a variável do Desktop; Clippy repetido e aprovado.
- git diff --check: aprovado; avisos LF/CRLF são conversão de final de linha.
- README: 17 blocos PowerShell analisados sem erro de sintaxe e 29 links locais verificados.
- Script sem modelo: scripts/probe-personal-protocol.mjs confirmou ambiente vazio e sandbox readOnly no mesmo lançador npm usado pelo adapter. Nenhum turno de modelo nem leitura da conta.

A revisão independente identificou e corrigiu um bloqueio de mensagens válidas com várias linhas ou Unicode: o título da conversa agora normaliza espaços/controles em uma linha e limita caracteres e bytes UTF-8. A mensagem original permanece intacta. Duas regressões cobrem a persistência real de mensagens com LF/CR/tab e emojis.

Três regressões finais verificam o bloqueio de Gemini/Copilot em 32 combinações de fornecedor/modo/escrita/retomada, a rejeição antes de resolver/criar workspace e a detecção do executável com caminho visível e chat indisponível.

## Isolamento de Gemini e Copilot

O pacote Gemini CLI 0.62.0 instalado confirmou que o ACP copia os MCPs de configuração herdada; mcpServers vazio não os desativa. Ferramentas confiáveis podem dispensar confirmação e hooks BeforeModel executam comandos sem passar por request_permission, mesmo no modo plan. Por isso o chat Gemini foi bloqueado em ambos os modos. Não foi executado hook, modelo ou ferramenta MCP para demonstrar esse caminho; a cadeia foi verificada no código instalado.

Copilot não estava instalado e seu isolamento de hooks/MCP não foi qualificado. A [documentação oficial de hooks](https://docs.github.com/en/copilot/reference/hooks-reference) distingue hooks de usuário/repositório e hooks de política administrativa. Não se presume que um flag desative todas essas superfícies. O chat também fica bloqueado antes de qualquer preparação ou processo.

A detecção, autenticação no terminal e observação de atividade das CLIs são independentes desse bloqueio. Configurações globais não foram editadas e o código ACP permanece disponível para qualificação futura. Preencher projeto, permitir escrita ou fornecer ID de retomada não contorna a guarda.

## Interface contínua no navegador

Evidência atual: windows/target/frontend-continuo/VALIDACAO.md e sete capturas JPEG. A prévia usa somente dados simulados; não inicia CLIs, lê arquivos pessoais nem envia mensagens a outros chats.

Foram conferidos troca de personagem conservando rascunho/anexos/resposta anterior, resposta simulada de outro fornecedor na mesma timeline, confirmação externa literal, negação da confirmação ao trocar de personagem sem reenvio, memória automática com origem/correção/exclusão e clique no personagem da visão geral abrindo o canal pessoal.

Abrir um canal avançado exibiu a confirmação. Ela não foi aceita. A ferramenta de navegador não conseguiu dispensar/fechar essa aba após o diálogo; seu cancelamento e limpeza não são classificados como PASS. O servidor de prévia foi encerrado. As capturas antigas em frontend-smoke descrevem a interface anterior e não qualificam o fluxo atual.

## Codex real: duas mensagens em sessão própria do teste

Comando opt-in: COUCOU_RUN_PERSONAL_SMOKE=1, seguido de cargo test -p coucou --lib personal_tools_are_scoped_and_resume_without_a_native_environment -- --ignored --nocapture.

Resultado final: um teste aprovado, zero falhas; dois turnos completados em 33,98 segundos. Login ChatGPT e fornecedor OpenAI exigidos pela fixture; chaves de API e fornecedores alternativos são recusados.

1. Pasta isolada com três arquivos diretos, um marcado oculto no Windows, e uma subpasta. Uma autorização de contagem exata; resposta correta de três arquivos e uma subpasta.
2. Reinício do app-server e retomada do mesmo ID nativo. Segunda contagem reutilizou apenas a concessão existente. A tentativa de ler outro arquivo pediu autorização e foi negada. O conteúdo canário não apareceu.
3. thread/read confirmou environments vazio depois de cada resposta, incluindo a retomada. A coleta reportou cota observada da conta. A sessão criada pelo teste foi arquivada pelo próprio teste.

O lançador efetivamente usado foi C:\Users\FortaTech\AppData\Roaming\npm\codex.cmd, pelo pacote oficial @openai/codex 0.159.2, iniciado com node.exe. A inicialização reportou Codex Desktop/0.159.2; o prefixo do userAgent é a origem da chamada e não identifica por si só a instalação Desktop. Uma inicialização sem CODEX_INTERNAL_ORIGINATOR_OVERRIDE, sem conta ou turno, confirmou coucou_actual_probe/0.159.2. A guarda valida a versão exata independentemente desses prefixos e rejeita versões ambíguas. Forçar codex.exe em um script de diagnóstico selecionou outro executável, 0.159.0-alpha.12.1; essa versão não recebeu a qualificação final.

A causa da falha de retomada foi o contrato do protocolo: ThreadResumeParams não tem environments, enquanto TurnStartParams aceita o override antes da entrada do modelo. O adapter valida identidade/pasta original antes de retomar, exige que a resposta inicial tenha ambiente vazio e aceita na retomada somente o ambiente local original. Cada turno pessoal envia environments vazio e não envia cwd. O contrato experimental permanece limitado à versão qualificada. Fontes: [protocolo de thread](https://raw.githubusercontent.com/openai/codex/rust-v0.159.2/codex-rs/app-server/src/request_processors/thread_processor.rs), [início de turno](https://raw.githubusercontent.com/openai/codex/rust-v0.159.2/codex-rs/app-server/src/request_processors/turn_processor.rs) e [seleção do ambiente](https://raw.githubusercontent.com/openai/codex/rust-v0.159.2/codex-rs/core/src/environment_selection.rs).

Tentativas intermediárias identificaram override com aspas literais em MCP e interpretação incorreta da concatenação de comentário mais JSON final. Foram corrigidas e as sessões pertencentes exclusivamente às fixtures foram arquivadas sem orientar conversas do usuário. A configuração global das CLIs não foi alterada.

## Worker de documentos

O worker dos executáveis debug e release passou dez casos reais em cada rodada: UTF-8/JSON/DOCX/PDF com referências, formatos malformados/sem texto, documento alterado por hash e segredo depois do orçamento. O harness foi atualizado para aceitar somente os executáveis conhecidos em target/debug e target/release; continua recusando os executáveis da raiz e instalados. A repetição contra o release final, incluindo a guarda de fornecedores, passou dez casos, zero falhas, sem GUI ou modelo.

Os limites Windows Job e encerramento por cancelamento/timeout possuem testes próprios. O probe de worker registra productionJobQualified=false: sua interrupção de stdin/deadline é feita pelo harness e não comprova os limites do Job de produção.

## Pacote e instalação

O comando npm run pack concluiu com exit 0. O build final gerou backend, frontend e instalador NSIS x64 0.1.2 em 5m58s na etapa Rust release. O instalador inclui PortugueseBR, English e French conforme os [recursos oficiais do Tauri](https://github.com/tauri-apps/tauri/blob/dev/crates/tauri-bundler/src/bundle/windows/nsis/languages/PortugueseBR.nsh). Gerar um instalador não comprova instalação, upgrade, login ou comportamento da janela nativa.

| Artefato local | Tamanho | SHA-256 |
|---|---|---|
| windows/release/Coucou-Windows-0.1.2-setup.exe | 5.675.381 bytes (5,41 MiB) | B98D07CB7A31823C62BCECBE7C3EFDF024EEF1DE06551E76396505FC5A8405FA |
| windows/target/release/coucou.exe | 10.804.736 bytes | 3909E1784363B13FDC3635368631B8733028C5FDF733D16C1CB3C94D044460D0 |

Geração final: 02/10/2026, 19:24:18 (America/Sao_Paulo). O nome rolling Coucou-Windows-setup.exe foi atualizado pelo empacotamento; o instalador 0.1.1 foi preservado. Os três hashes dos executáveis originais da raiz permanecem iguais aos registrados antes da implementação. Artefatos de build, dados pessoais, node_modules e executáveis da raiz não entram no commit.

Guia passo a passo: [README-WINDOWS.md](../README-WINDOWS.md). O aplicativo instalado não exige ferramentas de compilação; as CLIs e seus logins são separados. Os executáveis antigos da raiz não substituem o instalador novo.

## Comunicação externa e preservação

A [política de comunicação entre chats](CONTROLE-DE-MENSAGENS-ENTRE-CHATS.md) exige autorização humana para cada mensagem com destino e conteúdo completos. Não foi enviado prompt ao chat Retomar pareamento QR por PIN, nem criada automação substituta. A tentativa de atualizar o heartbeat encontrou registro inexistente; não se declara pausa de automação ausente.

A revisão automática bloqueou a limpeza de uma fixture temporária antiga, inclusive com lista exata de arquivos. Não apresentou justificativa além de blocked by policy. A remoção não foi executada e a pasta coucou-personal-smoke-30252-1790961925626290700 ficou preservada. Não houve tentativa de contornar o bloqueio.

## Dependências e limites práticos

SQLite bundled/FTS guarda histórico e contexto localmente; sha2 verifica integridade e recibos. lopdf, zip e quick-xml fazem extração local reduzida, sem Office, macros, links remotos ou execução de documentos. As dependências constam do lockfile. Não há runtime Hermes embutido nem cópia de código; a inspiração conceitual foi consultada na revisão c67b0a1dc4a08fe5437f5cb6e1391cbb864bcbf6.

Ainda devem ser conferidos manualmente no Windows:

1. Instalar o pacote, autenticar e enviar pelo personagem Codex sem selecionar projeto.
2. Perguntar sobre Downloads e conferir a autorização real; negar, autorizar a operação exata, revogar e repetir.
3. Arrastar documentos pela janela nativa, conferir cobertura/referências e preservar histórico.
4. Conversar sobre uma preferência, conferir sua origem no painel Contexto, corrigir/excluir e reiniciar. Revisar o diff de um procedimento antes de aprovar.
5. Conferir autorização por envio apenas em uma sessão externa de teste, sem mensagens repetidas.
6. Homologar topo central, cursor, foco, fixação, tray, drop OLE, DPI 100/125/150/200% e monitores com coordenadas negativas.

Esquecer dados no Coucou remove sua recuperação local. Não apaga automaticamente logs/sessões dos fornecedores, mensagens no aplicativo Codex ou documentos originais. Memórias, procedimentos, histórico e cópias de anexos têm controles próprios.
