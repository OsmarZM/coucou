# Validação: Hermes, FreeLLMAPI e anexos — Coucou Windows

**Período:** 06–08/10/2026, America/Sao_Paulo. Fechamento em 08/10/2026.

**Versão da implementação:** Windows 0.1.4, branch `codex/multiagent-windows`.

**Base preservada:** `682af47e46d3eb541334a093c22ceae58409a6c1`, que incorporou o oficial mantendo a implementação pessoal Windows.

**Estado do relatório:** fechamento da qualificação local. As verificações sem resultado consolidado estão identificadas abaixo; a existência do código não equivale a homologação com modelo remoto ou instalação real.

## Objetivo

Registrar o que esta entrega implementa, o que cada evidência demonstra e os gates que ainda exigem ambiente nativo ou conta de provedor. O [plano](PLANO-HERMES-ORQUESTRADOR-E-FREELLMAPI.md) descreve também evoluções posteriores; o [ADR](ADR-001-HERMES-COMO-SUPERVISOR.md) registra as decisões da primeira adaptação.

## Comportamento implementado

| Área | Comportamento no código | Limite material |
| --- | --- | --- |
| Seleção | Personagens na guia principal; identidade ativa no chat; lista/barra duplicada removidas | Hermes coordena apenas o canal Hermes; Codex/Claude mantêm canais diretos |
| Contexto | Timeline, rascunho, documentos e memórias pessoais compartilhados entre canais compatíveis | Sessões técnicas, login, cotas e permissões permanecem distintos |
| Importação | Identificador próprio, estado ocupado até conclusão IPC, erro persistente, cancelamento e prazo de 60 segundos | Teste do serviço/parser não demonstra entrega de arquivo pelo drag/drop OLE nativo |
| Leitura | Anexar localmente é separado de ler/compartilhar; ações explícitas para resumir/perguntar | OCR/visão, PDF só digitalizado e DOC legado não estão qualificados para leitura |
| Hermes | Adapter ACP do runtime real, perfil exclusivo, revisão de origem e manifesto efetivo antes do prompt; exige contexto configurado de 64.000 a 1.000.000 tokens | A capacidade deve ser confirmada na documentação do modelo; configuração disponível não comprova autenticação, inferência ou tool calling do provedor |
| Ferramentas | Oito operações MCP tipadas, conferidas no agente efetivo; callbacks nativos externos à ponte recusados | ACP/MCP não são sandbox de sistema operacional; preserva-se a política restrita do broker |
| Delegação | Até dois auxiliares Codex/Claude por turno, profundidade um, sessão nova somente leitura, aprovação do objetivo integral | Execução sequencial nesta entrega; não retoma chats externos nem muda os planos/logins das CLIs |
| Aprendizado | Hermes propõe memória/procedimentos; Rust/SQLite aplica política de origem e é escritor persistente único; snapshot espelhado no perfil | Ferramentas nativas memory/skills do Hermes estão desabilitadas; não se anuncia seu aprendizado nativo completo |
| FreeLLMAPI | Opt-in, Node privado, conta Groq/Cerebras autorizada e modelo exato; autenticação e identidade em loopback | Sem `auto`, fallback, saldo pessoal de bilhões ou garantia de cota gratuita |
| Inicialização | Preferência própria de iniciar servidor após login, independente de iniciar Coucou | Não inicia agente, inferência, retomada ou mensagens no boot; login/upgrade reais exigem gate próprio |
| Métricas | Eventos de supervisor/auxiliares separados; processos e atividades observados; tokens Hermes identificados como informação parcial | ACP não preserva a origem estimada do FreeLLMAPI; capacidade configurada não é medição de ocupação; cota/reset ausentes continuam desconhecidos |

O original do documento permanece intacto. O armazenamento aceita até 10 anexos por contexto, 20 MiB por arquivo e 50 MiB por lote. A cópia importada e os limites do trecho preparado são controlados pelo serviço de documentos. Conteúdo de documento ou resposta de executor não concede acesso, autostart ou autorização de mensagem.

## Runtimes e integridade

| Componente | Revisão/versão fixada | Preparação |
| --- | --- | --- |
| Hermes Agent | `781334eea4b9225a3e194faf0c241d9afe218634` | `windows/scripts/managed-services/provision-hermes.ps1`; Python/ACP/MCP privados; hashes da origem/locks e 36 arquivos críticos conferidos |
| Python Hermes | `3.14.2`, instalação gerenciada por `uv 0.9.26` | Destino exclusivo; ambiente global preservado; caminho final `runtime/source/.venv/Scripts/python.exe` |
| FreeLLMAPI | `a6b2158c7c36ce19f888d0411846a1a0f2aa3f06` | `provision-free-models.ps1`; hash de arquivo/lockfile, dependências fixadas e recibo protegido de manifesto |
| Node do roteador | `22.22.0`, Windows x64 | Arquivo e executável com SHA-256 fixado; Node global não é usado para executar o serviço |

Downloads acontecem apenas na instalação explícita. Nenhum setup/atualização automática é iniciado no login. Os destinos já existentes e instalações parciais são preservados para diagnóstico. O gerenciador recusa manifesto modificado, recibo ausente, origem incompatível e perfil Hermes não pertencente ao Coucou.

## Ponte de ferramentas e autorização

As operações expostas são `context_read`, `files_request`, `files_count`, `files_list`, `document_read`, `memory_propose`, `skill_propose` e `delegate_cli`. O perfil recusa terminal, código, plugins, MCP herdados e as ferramentas nativas de delegação/memória/skills. O catálogo genérico de descoberta de ferramentas fica desligado em `tools.tool_search.enabled`; título automático e inferências auxiliares nativas são bloqueados. O carregamento de overlay global e a atualização automática da superfície MCP também ficam desabilitados. Uma ferramenta efetiva adicional, ausente ou duplicada impede o prompt.

O broker limita o turno a 16 chamadas de ferramentas e duas delegações. IDs de requisição consumidos não são repetidos. Cada auxiliar recebe objetivo integral e contexto do turno como dados externos, sem poder escolher shell, raiz de arquivos, sessão externa, escrita ou nova delegação. A aprovação permite uma delegação; não vira autorização permanente. Cancelar o turno sinaliza os processos filhos pertencentes ao mesmo escopo.

Chats externos continuam sujeitos à [política de comunicação](CONTROLE-DE-MENSAGENS-ENTRE-CHATS.md): destino e mensagem integrais, aprovação única e nenhuma repetição automática em timeout ou resultado incerto. A ponte Hermes não oferece uma operação para orientar chats existentes. Não foram criadas automações ou heartbeats para contornar essa regra.

## Serviço gratuito e dados

O bootstrap gerenciado inicializa somente o servidor necessário ao proxy. Ele não executa o desktop, o entrypoint genérico com jobs/cron, downloads ou testes de modelo no boot. Uma chave de transporte local e prova de identidade precedem o uso do endpoint; um PID/arquivo de porta não basta para confiar no serviço. Credenciais não são devolvidas à WebView nem enviadas a um processo estranho que ocupe uma porta.

A configuração permite apenas Groq/Cerebras autorizados e um modelo exato. Chaves são guardadas no Windows Credential Manager, com namespace próprio. A chave mestra do banco é persistente; banco existente sem chave mestra é recusado, sem geração silenciosa de substituto. Configuração/saldo anunciado não é garantia de que uma conta tem cota gratuita. A execução delegada ainda consome o plano da CLI correspondente.

Dados ficam em `%LOCALAPPDATA%\Coucou`: `personal\state.db`, `documents`, `managed-services\hermes` e `managed-services\free-models`. O perfil integrado Hermes fica em `managed-services\hermes\profiles\personal-main`. Fonte, scripts e testes são portáveis pelo Git; dados pessoais, runtimes provisionados e credenciais não vêm no clone.

## Evidência local desta entrega

| Verificação | Estado no fechamento | O que demonstra |
| --- | --- | --- |
| Frontend — `npm test` | PASS em 08/10: 122 passaram, zero falhas; um teste de integração opt-in separado | Contratos de estado/contexto, seleção, anexos, envio explícito, desabilitação com autostart e proxy gerenciado, em testes/fixtures |
| Rust — `cargo test --workspace` | PASS em 08/10: 158 passaram, zero falhas; três testes reais opt-in ignorados | 137 do aplicativo, 12 do protocolo, seis do hook e três de neutralidade; recusa de rotas/configuração, atestação, IDs consumidos, isolamento de imports, limites e contratos do broker |
| Política bootstrap Hermes — Python | PASS: 13 testes em 08/10 | Ferramentas extras/nativas e lotes mistos, rota/fallback, perfil/overlay herdado, inferência auxiliar, comando MOA, contexto insuficiente, repetição após falha e clientes com timeout/retries divergentes recusados; sem modelo remoto |
| Hermes real + ACP/MCP + modelo local | PASS em 08/10: atestação exata, leitura e streaming; três chamadas de modelo e uma de broker | Runtime e SDK reais com fixtures de modelo/broker em loopback; sem inferência remota, CLI auxiliar ou aprovação na janela |
| TypeScript/frontend/fmt/clippy | PASS em 08/10: `tsc --noEmit`, Vite, `cargo fmt --all --check` e Clippy com `-D warnings` | Compilação do frontend e análise estática da implementação integrada; pacotes de release são registrados separadamente |
| Worker de documentos no executável gerado | PASS em 08/10: dez casos repetidos no executável final 0.1.4 | UTF-8/JSON/DOCX/PDF textual, recusas de arquivo inválido/credenciais/integridade/imagem e encerramento pelo harness; separado do drag/drop da janela |
| Pacotes NSIS/MSI 0.1.4 | PASS em 08/10: `npm run pack`, dois pacotes gerados e cópias conferidas por SHA-256 | Geração local e arquivos produzidos; não prova instalação/upgrade |

Os resultados consolidados abaixo complementam a tabela. Resultados das versões anteriores permanecem no [relatório pessoal](VALIDACAO-AGENTE-PESSOAL-WINDOWS-2026-10-02.md) e no [registro do oficial](INTEGRACAO-OFICIAL-2026-10-06.md); não são reutilizados como homologação automática desta integração.

### Hermes real com modelo e broker locais

O probe estrito em `windows/scripts/managed-services/probe-hermes-runtime.py` executou o Python/ACP/MCP reais da revisão fixada e o transporte Rust do Coucou. A repetição com o executável final, em `windows/target/hermes-runtime-probe-20261008-release-final/evidence.json`, registra PASS: duas chamadas no turno normal (consulta de contexto e resposta), uma na tentativa de ferramenta nativa proibida, três no total e uma chamada `context_read` ao broker. As oito ferramentas efetivas foram conferidas também nos pedidos recebidos pelo modelo. Houve streaming de resposta, recusa de troca de modelo/modo, slash command e payload MOA antes de consultar o modelo, recusa de terminal antes do dispatch e bloqueio da sessão após a falha, sem novo envio. `modelFailures` ficou vazio e o processo saiu com código zero.

O cliente SDK real passou pelo gate de URL/chave próprias, `max_retries=0` e timeout efetivo de conexão/leitura/escrita/pool positivo e até 180 segundos. O prazo geral do turno permanece em 600 segundos, com cancelamento do escopo de processos. O bootstrap final desta prova tem SHA-256 `f3a3983d97ce541f810562700e50c6ae179e901af56df0fd3f2880dd9294602b`. A repetição final durou aproximadamente 54 segundos: `initialize` em 16,397 segundos e `session/new` em 24,554. A primeira prova estrita também passou, em cerca de 59 segundos. Sob concorrência de disco, uma inicialização anterior levou 251,234 segundos; o RPC de inicialização admite até 300 segundos. Esses tempos não representam a latência de um provedor remoto.

O executável final usado no transporte tem SHA-256 `fb1ccb114c50f914ad036bd711a1d85f4ec2d56853b64e84dfefcbe35343e3a4`, correspondente ao binário gerado em 08/10. As fixtures usam perfil, chave e dados próprios de teste, sem consultar logins globais ou chats existentes. O harness inicia o adaptador diretamente; os contratos Rust de preparação e execução têm testes unitários, mas o fluxo completo do supervisor pela janela ainda exige homologação própria. Isso qualifica o transporte e a política observados; não homologa conta Groq/Cerebras nem execução de um auxiliar Codex/Claude. A evidência intermediária `f` não é usada como gate final: apesar das recusas de ferramentas, revelou oito consultas repetidas do modelo após falha. O gate estrito e o corte do loop foram acrescentados antes dos probes finais.

### Executável e pacotes finais

`npm run pack` concluiu em 08/10 e produziu NSIS e MSI. O executável informa versão `0.1.4`; os nomes versionados e os aliases em `windows/release/` têm hashes iguais aos pacotes de `target/release/bundle/`.

| Arquivo | Bytes | SHA-256 |
| --- | ---: | --- |
| `windows/target/release/coucou.exe` | 11.830.272 | `fb1ccb114c50f914ad036bd711a1d85f4ec2d56853b64e84dfefcbe35343e3a4` |
| `windows/release/Coucou-Windows-0.1.4-setup.exe` | 6.371.678 | `3f0076c03778d78db085048f3276f52ac73342b273a6d8eb63c3bff9973eb270` |
| `windows/release/Coucou-Windows-0.1.4.msi` | 7.614.464 | `389a2d75bc567d72152d5a3d78e350585cb62f150761fe4c4d681a5ac21cfe5f` |

Os dez casos do worker foram repetidos contra esse executável final, sem GUI ou chamada de modelo. Os recibos `windows/target/validation-final-20261008/rust-pack-checks.json` e `rust-pack-final.json` registram todas as saídas zero e fechamento `passed=true`. Clippy com `-D warnings` passou; a linkedição de release mostrou apenas o texto do MSVC sobre criação da biblioteca de importação, sem impedir o empacotamento.

Os três executáveis preexistentes na raiz foram preservados. Instaladores, runtimes e recibos locais permanecem fora do Git; o workflow de publicação continua pausado com `PUBLISH=false`. Esta prova não inclui instalação, upgrade, assinatura nem inicialização real do Windows.

### FreeLLMAPI real com provedor local de teste

Na pasta `windows`, foi executada a suíte opt-in com `COUCOU_TEST_FREE_RUNTIME` apontando para `target\validation-free-models\runtime-a6b2158c7c36ce19f888d0411846a1a0f2aa3f06`:

```powershell
node --test tests/managed-free-models.test.mjs tests/managed-free-models.integration.test.mjs
```

PASS: cinco testes, zero falhas/ignorados, aproximadamente 82,5 segundos. A fonte real fez quatro encaminhamentos para o stub Groq local, um cancelamento observado, zero consultas de provedor no boot e zero chamadas a provedor não autorizado. Inicialização dos dois processos: 27.278 e 29.920 ms. A suíte verificou identidade/autenticação, recusa de origem de navegador e de namespace/modelo diferente, HTTP 200, streaming SSE com `usage.estimated=true` e `[DONE]`, concorrência, cancelamento, bloqueio após interrupção e uma única chamada em erro de cota, sem retry/fallback. Recibo local ignorado pelo Git: `windows/target/managed-free-qualification-dUtKcO/qualification.json`. Ambos os processos foram encerrados.

O utilitário independente `node scripts/managed-services/verify-free-runtime.mjs <runtime>` conferiu os 14.231 arquivos do manifesto: PASS. SHA-256 da ponte conferida: `902d82d7008322a53f370434f1aa8c475338951b473b9743a6418c6b2ee318df`. O teste não registrou recibo no Credential Manager nem alterou HKCU/autostart. Usou somente credenciais fictícias e inferência interceptada para loopback; não homologa Groq/Cerebras remotos.

O provisionamento frio isolado demorou aproximadamente 48 minutos sob concorrência de disco. A conferência independente Node levou 665.478 ms, cerca de 11 minutos e cinco segundos; esse número não é benchmark do verificador Rust do aplicativo. Setup admite até 60 minutos e a prontidão do processo até 120 segundos, depois da verificação integral de arquivos. A interface informa a operação demorada. Integridade não foi reduzida para obter um PASS.

### Janela nativa: seleção e anexo TXT

Em 07/10, o primeiro executável 0.1.4, SHA-256 `c0381eb38f2a99ee262fcf46419a0520b2015385918ec24418815d4b73618f8f`, foi aberto para esta prova. O personagem Hermes na guia principal abriu o chat Hermes sem lista de fornecedores/conversas. A ausência do runtime de produção mostrou motivo e impediu o envio, preservando a importação local.

Pela guia **Anexar → Escolher arquivos**, foi selecionado `leitura de teste.txt`, um TXT UTF-8 sintético com espaços no caminho, criado no diretório de validação. A janela apresentou importação e depois **anexado localmente / leitura pendente**, com ações **Ler e resumir** e **Fazer uma pergunta**. Nenhuma ação de envio ao modelo foi executada. O banco pessoal foi copiado para backup privado antes da prova; o documento sintético foi mantido para inspeção. O processo aberto para a prova foi encerrado antes da reconstrução final.

Essa prova não qualifica drop OLE, nomes acentuados, PDF/DOCX pelo seletor, clique de cancelamento, permissões nativas ou modelo remoto. O widget recolheu para o estado compacto visível após perder foco; esta prova não é homologação abrangente de hover/DPI/múltiplos monitores.

## Gates nativos e de provedor

| Gate | Estado | Critério para homologar |
| --- | --- | --- |
| Importação pela janela Tauri | Parcial: TXT pelo seletor nativo confirmado em 07/10 | Restam drop OLE, PDF textual/DOCX pelo seletor, nomes acentuados, cancelamento, limite e erro nativos |
| Hermes + Groq | Não homologado com conta/modelo real | Pergunta mínima explícita, streaming, chamada de ferramenta e retomada/cancelamento no perfil gerenciado |
| Hermes + Cerebras | Não homologado com conta/modelo real | Mesmo gate, qualificado independentemente para o modelo da conta |
| Auxiliar Codex/Claude iniciado por Hermes | Não homologado com modelo real nesta integração | Negar não cria processo; permitir cria uma sessão nova; resultado aparece no supervisor; cancelamento alcança o filho próprio |
| CLI Codex pessoal anterior | Evidência histórica em 0.159.2; não substitui o gate Hermes | Dois turnos/retomada/escopo já registrados; aprovações eram simuladas pelo smoke anterior |
| Claude direto | Sem turno remoto homologado nesta entrega | Confirmar login, resposta e cancelamento com a CLI efetivamente selecionada |
| Gemini/Copilot | Chat bloqueado | Qualificar ausência efetiva de hooks/MCP herdados antes de habilitar |
| Login do Windows com autostart gratuito | Não homologado por reinicialização real | Servidor próprio volta uma vez, sem prompt/tarefa; desativação remove apenas sua entrada; app autostart preservado |
| Instalação, upgrade e outra máquina | Não homologado | Instalar pacote gerado, preservar dados/hooks, provisionar runtimes, autenticar e verificar versão/configuração |

Não foram usados logins/chaves globais para anunciar conexão nova como pronta. Uma inferência de fixture ou endpoint local de teste demonstra o transporte do runtime, não a capacidade/comportamento do provedor remoto. Uma aprovação simulada demonstra o contrato, não o clique humano na janela nativa.

## Como reproduzir

Siga o [README Windows](../README-WINDOWS.md) para clonar o branch completo, instalar pré-requisitos, compilar, provisionar runtimes opcionais e configurar suas próprias contas. Na pasta `windows`:

```powershell
npm test
cargo test --workspace
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
npm run pack
node scripts/probe-documents-worker.mjs --executable=./target/release/coucou.exe
```

Para a política Python, use o Python privado depois de provisionar Hermes:

```powershell
$hermesPython = Join-Path $env:LOCALAPPDATA 'Coucou\managed-services\hermes\runtime\source\.venv\Scripts\python.exe'
& $hermesPython -I .\scripts\managed-services\test-hermes-bootstrap.py
```

Os testes de política não iniciam modelo remoto. Turnos reais, permissões nativas, autostart/login e instalação requerem ações explícitas no aplicativo e evidência separada. Não desative verificações de integridade ou isolamento para converter um erro em disponibilidade.
