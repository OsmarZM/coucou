# Plano de evolução do Coucou para múltiplos agentes no Windows

**Data:** 01/10/2026

**Status:** base, adapters e configuração das quatro CLIs implementados; homologação real por superfície pendente

**Projeto:** `D:\coucou`, versão Windows
**Base inspecionada:** branch `main`, commit `5ae7bd9`, com alterações locais preexistentes

## 1. Objetivo e decisão de escopo

Ampliar a ilha do Coucou para acompanhar Claude Code, Codex CLI, Gemini CLI e GitHub Copilot CLI, identificando agente, sessão e projeto. A experiência deve mostrar atividade, conclusão, erro e necessidade de ação humana sem interferir no funcionamento normal das ferramentas.

Premissa deste plano: a primeira entrega é o acompanhamento das sessões que o usuário já utiliza no Windows. O chat com vários provedores é uma evolução independente. A escolha entre CLI, aplicativo desktop e extensão do VS Code ainda precisa ser confirmada para cada ferramenta; o suporte inicial proposto é às CLIs, onde os mecanismos de eventos são mais explícitos.

### Resultado esperado

- Claude, Codex, Gemini e Copilot aparecem com seus nomes corretos.
- Duas sessões do mesmo agente, inclusive no mesmo projeto, têm estados independentes.
- A ilha informa a última atividade disponível e a conclusão do turno.
- O usuário instala, verifica e remove cada integração por um fluxo reversível.
- Os pedidos de aprovação existentes do Claude continuam funcionando; aprovações para novos agentes entram somente após qualificação do protocolo específico.
- As integrações atuais de serviços e a aparência do Coucou continuam compatíveis.

### Prioridades

| Prioridade | Entrega | Motivo |
|---|---|---|
| P0 | Acompanhamento local das quatro CLIs, sessões independentes e configuração reversível | Resolve o uso principal proposto. |
| P0 | Preservar o Claude e suas aprovações atuais | Evita regressão na integração funcional do projeto. |
| P1 | Aprovações de outros agentes, onde houver protocolo suportado e homologado | Depende de semânticas diferentes entre ferramentas. |
| P1 | Melhor navegação para o terminal/editor de origem | A disponibilidade varia conforme a superfície utilizada. |
| P2 | Codex desktop e extensões de agentes no VS Code | Exigem investigação e homologação próprias. |
| P2 | Chat interno com vários provedores | Tem contratos, autenticação e consumo separados do monitoramento. |

MacOS, execução de agentes pelo Coucou, roteamento automático entre modelos, histórico persistente de conversas e novas integrações de serviços ficam em iniciativas próprias. Essa delimitação mantém a primeira entrega pequena e verificável.

### Casos de uso

- Como desenvolvedor, quero identificar qual agente e projeto estão trabalhando para acompanhar minhas sessões sem alternar entre terminais.
- Como desenvolvedor com sessões simultâneas, quero selecionar cada sessão para ver a atividade correta sem misturar seus estados.
- Como usuário do Coucou, quero instalar e remover uma integração com prévia e backup para preservar minhas configurações existentes.
- Como usuário de uma integração com aprovação suportada, quero saber qual ação estou autorizando e devolver a decisão ao terminal quando não responder na ilha.

## 2. Diagnóstico do código atual

| Área | Evidência no projeto | Consequência para a evolução |
|---|---|---|
| Stack | `windows/package.json`, `windows/Cargo.toml` | Tauri 2, Rust, TypeScript e Vite; manter a stack atual. |
| Identidade | `windows/src/core/state.ts` | `AgentSource` aceita `claudeCode` e `n8n`; os eventos do Claude compartilham `integration_claude`. |
| Sessões | `windows/src/island/hooks.ts` | Recebe `session_id`, mas atualiza a mesma tarefa fixa; a conclusão usa um timer que precisa ser vinculado ao turno correto. |
| Aprovações | `windows/src/core/state.ts`, `windows/src/island/hooks.ts` | Existe um único `pendingApproval`; um segundo pedido é devolvido ao fluxo nativo, sem substituir o primeiro. |
| Transporte | `windows/src-tauri/src/pipe.rs` | Named pipe local por usuário; o backend reconhece diretamente `PermissionRequest` do Claude. |
| Relay | `windows/hook/src/main.rs` | Binário dedicado ao Claude, com saída específica para seu protocolo e limites de tempo. |
| Instalador | `windows/src-tauri/src/hooks.rs` | Já tem prévia, backup, detecção de mudança desde a prévia e remoção das próprias entradas; generalizar por agente. |
| Preferências | `windows/src-tauri/src/settings.rs`, `windows/src/core/state.ts` | O estado dos hooks é um booleano; trocar por configuração por agente com migração compatível. |
| UI | `windows/src/views`, `windows/src/settings/main.ts`, `windows/src/island/island.ts` | Há caminhos especiais para `integration_claude` e o nome `VS Code`; explicitar agente e origem separadamente. |
| Limite visual | `windows/src/core/state.ts` | Existe limite de quatro integrações opcionais de serviços; as sessões de agentes precisam de agrupamento próprio. |
| Chat | `windows/src-tauri/src/claude.rs` | Cliente Anthropic separado do fluxo de hooks; preservar na primeira entrega. |

O nome `GitHub` de uma integração de serviços existente não representa o GitHub Copilot. A nova integração deve ter identificador e apresentação distintos.

### Reaproveitamento do Agent Island

O projeto relacionado `D:\Projetos MVP\Agent Island` já contém implementações candidatas de protocolo, adapters e instalador para quatro agentes. Usar esse código como referência pode reduzir trabalho, principalmente em normalização de eventos, instalação reversível e testes de falhas.

A decisão proposta continua sendo evoluir o Coucou. Antes de portar um componente, conferir o estado local do Agent Island, a licença, suas dependências e os contratos. Seu frontend React e sua persistência não justificam trocar o TypeScript atual do Coucou ou introduzir um banco de dados para esta entrega.

Código existente e testes sintéticos no Agent Island são evidência de implementação, não homologação das integrações no Coucou. Cada adapter portado passa pelos critérios deste plano.

Candidatos concretos: `crates/core/src/adapters.rs`, `protocol.rs`, `redact.rs`, `ipc.rs` e `crates/hook-client/src/installer.rs`, no Agent Island. Extrair apenas os módulos e testes necessários: a crate inteira inclui persistência e cliente JEV. Adaptar namespaces de pipe, diretórios, protocolo e assinaturas para o Coucou.

A inspeção encontrou alterações staged no Agent Island; portar a partir de uma revisão explícita, preservando esse trabalho. A atribuição MIT do código Coucou está registrada ali, mas não foi encontrada uma licença explícita para os módulos próprios do Agent Island. Definir autoria/licença desses módulos antes de sua distribuição pública; não presumir que a licença do Coucou se estende automaticamente a eles.

## 3. Matriz inicial de suporte

A inspeção local identificou Codex CLI **0.159.2**, com `hooks stable true` no inventário de features. Gemini e Copilot CLI não foram encontrados no PATH. Essa verificação não iniciou sessões e não homologou nenhuma integração do Coucou.

| Agente/superfície | Monitoramento proposto | Aprovação | Evidência e limite |
|---|---|---|---|
| Claude Code | Migrar hooks já implementados para a base comum | Preservar a implementação atual e testar sua regressão | Integração existente no código Coucou; não foi repetida homologação nesta elaboração. |
| Codex CLI | Lifecycle hooks, com identificação de sessão/turno | `PermissionRequest`, em fase própria | CLI local e feature verificadas; adapter Coucou ainda não existe. [Documentação](https://developers.openai.com/en-US/docs/hooks). |
| Gemini CLI | `BeforeAgent`/`AfterAgent`, eventos de ferramentas, sessão e notificações | Alerta e encaminhamento à CLI; `Notification` não concede permissão | Documentação disponível; CLI ausente no PATH. [Referência](https://geminicli.com/docs/hooks/reference/). |
| Copilot CLI | Eventos de sessão, prompt, ferramentas e término do agente | `permissionRequest`, em fase própria e respeitando sandbox | Documentação disponível; CLI ausente no PATH. [Referência](https://docs.github.com/en/copilot/reference/hooks-reference). |
| Codex desktop/VS Code | Candidato a hooks do runtime local | Depende da superfície e versão homologadas | Prova real pendente; não iniciar segundo app-server e chamar isso de observação deste aplicativo. |
| Copilot no VS Code | Hooks conforme o target/harness selecionado | Varia conforme o harness | Hooks do VS Code estão em Preview; Local e Agent Host têm contratos distintos. [Referência](https://code.visualstudio.com/docs/agent-customization/hooks). |

Para Codex, distinguir `Stop` de `SessionEnd` e não inferir sucesso de ferramenta a partir de `PostToolUse`. A configuração pode ser descoberta em várias camadas; a confiança dos hooks não gerenciados é revisada no `/hooks`. A etapa de instalação deve explicar esse passo sem autorizar hooks pelo usuário.

Para os relays passivos, usar saída neutra e código de saída compatíveis com cada fornecedor. Silêncio, JSON neutro e código de erro não têm semântica universal. Nunca adicionar contexto, bloquear ferramentas ou emitir decisões como efeito colateral do monitoramento.

## 4. Arquitetura proposta

```text
CLI do agente
  -> hook oficial + coucou-hook
  -> adapter específico do agente
  -> evento normalizado e validado
  -> named pipe do Coucou
  -> registro de sessões e pedidos pendentes
  -> evento Tauri
  -> estado e interface da ilha

Clique humano em aprovação suportada
  -> requestId + sessão + agente
  -> pedido pendente no backend
  -> adapter converte a decisão para o protocolo da CLI
```

### Responsabilidades

1. **Adapter do agente:** conhece eventos, campos, capacidades e formato de resposta do fornecedor. A interface não interpreta payload bruto de cada CLI.
2. **Relay e transporte:** entregam eventos locais com limite de tamanho e tempo. A ausência do Coucou não cria uma autorização nem impede a CLI de seguir seu fluxo nativo.
3. **Registro de sessões:** organiza sessões, turnos, atividade recente, expiração e deduplicação conforme os identificadores disponíveis. Define teto de sessões, quantidade/tamanho de resumos e prazo de retenção em memória; limpa sessões encerradas e estados antigos de forma previsível.
4. **Backend de aprovações:** correlaciona pedidos, valida decisão de uso único e descarta respostas expiradas. A UI apenas apresenta e envia o clique.
5. **Interface:** agrupa por agente, permite selecionar sessão e exibe só ações suportadas pela integração qualificada.
6. **Instalador:** conhece os arquivos e formatos por agente e preserva as configurações de terceiros.

### Contrato inicial a definir na fase 1

Os campos abaixo são uma proposta interna do Coucou, não campos garantidos nas APIs dos fornecedores:

| Campo | Finalidade |
|---|---|
| `protocolVersion` | Versionar o contrato entre relay e aplicativo. |
| `agent` | Identificar `claude`, `codex`, `gemini` ou `copilot`. |
| `sessionKey` | Identidade interna estável da sessão. |
| `nativeSessionId` | Identificador original, quando o fornecedor emitir. |
| `eventType` | Evento normalizado: início, atividade, ferramenta, fim do turno, erro ou fim da sessão. |
| `projectPath` | Projeto da sessão, quando disponível. |
| `source` | Origem conhecida: CLI, extensão ou aplicativo. |
| `turnId` / geração interna | Impedir que timer ou evento antigo altere um turno novo. |
| `requestId` / expiração | Correlacionar aprovação somente quando suportada. |
| `summary` | Metadados limitados da atividade, sem guardar payload bruto por padrão. |

A identidade da sessão usa o identificador nativo e seu domínio de validade. Se uma superfície não oferecer identificador confiável, a fase 0 precisa demonstrar uma correlação alternativa estável. Apenas usar `cwd` não distingue duas sessões no mesmo projeto. Sem correlação confiável, declarar a limitação e reduzir a capacidade exibida.

Capacidades como status, atividade detalhada, conclusão, aprovação e abertura da origem devem ser separadas. Hook instalado, CLI detectada e evento recebido são estados diferentes. Compatibilidade documental também não significa teste real aprovado.

Aplicar lista explícita de metadados permitidos e sanitização aos resumos/logs. Excluir deles prompts completos, transcripts, patches, resultados brutos e variáveis de ambiente. Manter apenas o contexto temporário necessário para apresentar uma aprovação, sem persistir seu payload bruto. A política e seus limites entram no contrato da fase 1.

### Estados de apresentação

Separar o estado do trabalho da evidência de recebimento. Mostrar: ocioso, trabalhando, aguardando ação, turno concluído ou erro; mostrar último evento como informação própria. Hooks são processos curtos: receber um evento comprova atividade recente, não uma conexão persistente ou a saúde atual da CLI. Reservar indicação de conexão para transportes que ofereçam essa evidência.

Mapear “pensando”, limite de uso e conclusão somente quando houver evento confiável. Inatividade ou timeout não significam sucesso. Falha de uma ferramenta não significa necessariamente falha da sessão inteira.

## 5. Sequência de entrega

| Fase | Trabalho | Entregável | Critério para encerrar |
|---|---|---|---|
| 0 — Qualificação | Confirmar superfícies utilizadas, versões, eventos, identificação de sessão e configurações; comparar o Agent Island | Matriz por agente, versão e superfície; decisão de reaproveitamento | Caminho de monitoramento definido e limitações declaradas para cada agente. |
| 1 — Base comum | Contrato, adapters, registro de sessões, capacidades e migração de preferências; converter o Claude | Claude funcionando sobre a base multiagente | Duas sessões simultâneas do Claude não se sobrescrevem; aprovações atuais e configurações antigas continuam funcionando. |
| 2 — Codex CLI | Adapter de hooks da versão qualificada, instalador e estado por sessão | Codex visível junto do Claude | Sessão real emite eventos esperados; fechamento do Coucou preserva o fluxo nativo; concluir turno A não altera turno B. |
| 3 — Gemini CLI | Adapter, mapeamento de eventos e instalação por formato próprio | Gemini visível na mesma ilha | Atividade e término do turno demonstrados em sessão real; observabilidade de notificações não é tratada como autorização. |
| 4 — Copilot CLI | Adapter, escolha explícita de escopo de instalação e tratamento de sobreposição de hooks | Copilot visível na mesma ilha | Eventos reais demonstrados; instalação em usuário/projeto não duplica o handler do Coucou inadvertidamente. |
| 5 — Experiência e entrega | Seleção de sessão, agrupamento, diagnóstico, acessibilidade, regressão visual e pacote Windows | Build candidato com matriz de homologação | Critérios de aceite aprovados nas superfícies anunciadas; limitações e versões registradas. |
| 6 — Aprovações adicionais | Qualificar cada protocolo; expiração, clique único e fallback | Aprovações habilitadas por capacidade comprovada | Permitir/negar/devolver ao fluxo nativo demonstrados na CLI real e cenários de falha cobertos. |
| 7 — Expansão | Investigar desktop/extensões e especificar chat com provedores | Especificações próprias por superfície/provedor | Viabilidade demonstrada antes de anunciar suporte ou alterar o fluxo de chat. |

A ordem proposta prioriza o Codex depois da base comum. Se a qualificação demonstrar um impedimento de versão ou superfície, adiantar outro adapter não muda os critérios de aceite. As fases são marcos de trabalho, não uma promessa de prazo; estimar esforço após a fase 0 e o baseline de build.

### Backlog concreto

- **P0-01:** matriz de eventos/capacidades por agente e versão, com fontes oficiais e amostras sanitizadas.
- **P0-02:** contrato Rust/TypeScript e validação do envelope; compatibilidade do relay antigo durante a transição.
- **P0-03:** registro por sessão, geração de turno e limpeza de timers ao encerrar/reiniciar.
- **P0-04:** adapter Claude e testes de regressão com pedidos simultâneos.
- **P0-05:** instalador por agente, configuração por escopo e migração de preferências.
- **P0-06:** adapter Codex e smoke real.
- **P0-07:** adapter Gemini e smoke real.
- **P0-08:** adapter Copilot e smoke real.
- **P0-09:** agrupamento de agentes/sessões e diagnóstico na interface.
- **P0-10:** pacote Windows, regressão visual e registro final de homologação.
- **P1-01:** aprovações adicionais individualmente qualificadas.
- **P2-01:** viabilidade de superfícies desktop/VS Code.
- **P2-02:** especificação do chat com vários provedores.

Cada adapter é uma mudança revisável, com código, configuração, evidência e limite de suporte próprios. A base comum precede sua implementação.

## 6. Fluxo de configuração e instalação

1. Selecionar agente e superfície.
2. Detectar a CLI e sua versão sem autenticar ou iniciar trabalho do agente.
3. Identificar caminho e escopo efetivo da configuração, incluindo sobreposição usuário/projeto.
4. Mostrar diagnóstico, capacidades previstas e prévia exata das mudanças.
5. Aplicar somente após o clique de instalação; preservar bytes originais no backup.
6. Abortar se o arquivo mudar depois da prévia ou tiver formato inválido.
7. Fazer substituição atômica e verificar as entradas instaladas; reinstalar sem duplicar ou reordenar hooks de terceiros.
8. Aguardar primeiro evento de uma sessão real para mostrar recebimento confirmado e a hora da última atividade.
9. Desinstalar removendo apenas entradas gerenciadas do Coucou; preservar modificações de terceiros posteriores à instalação.

A interface deve distinguir “CLI ausente”, “hooks configurados”, “aguardando sessão”, “evento recebido” e “configuração incompatível”. Um botão “verificar” não basta para marcar a integração como homologada.

Cada fornecedor recebe um instalador compatível com seu escopo e formato. Não assumir o caminho padrão quando a CLI permite uma pasta de configuração alternativa. Converter unidades de timeout por adapter: por exemplo, Gemini documenta milissegundos; Codex documenta segundos. Para Copilot, avaliar arquivo dedicado ao Coucou onde o formato qualificado permitir, facilitando remoção seletiva.

### Regras de aprovações

- Preservar o fluxo atual do Claude na migração.
- Novos agentes começam em observabilidade; habilitar controle somente por protocolo e versão homologados.
- Vincular a decisão a agente, sessão, pedido, prazo e instância do backend; rejeitar decisão antiga, duplicada ou incompatível.
- Exigir interação humana explícita no cartão correto; validar no backend a janela de aprovação ativa/focada e impedir que atalhos sem foco autorizem ações.
- Preservar inicialmente uma aprovação visível por vez. Outros pedidos seguem o fluxo nativo, sem substituir silenciosamente o cartão ativo.
- Encerrar espera ao pausar/fechar o aplicativo, perder o canal ou vencer o prazo. Falha ou silêncio nunca viram `allow`.
- Exibir exatamente o alvo de ferramenta disponível no evento, sem transformar conteúdo recebido em comando executável.
- Se houver semântica especial de sandbox, respeitar suas restrições e testar o caminho específico.

## 7. Critérios de aceite e validação

### Funcionalidade

- [ ] As quatro CLIs têm integração qualificada na versão/superfície declarada, ou são identificadas explicitamente como pendentes na entrega parcial.
- [ ] Dois agentes simultâneos não alteram o projeto, estado ou histórico um do outro.
- [ ] Duas sessões do mesmo agente no mesmo diretório permanecem distinguíveis.
- [ ] Eventos repetidos, atrasados e timers de turno antigo não limpam o estado do turno novo.
- [ ] Registro de sessões e resumos respeita os limites de memória/retenção; logs e resumos não armazenam os conteúdos excluídos pela política de dados.
- [ ] Conclusão do turno é apresentada somente após evento compatível; timeout informa ausência de sinal.
- [ ] Selecionar agente/sessão mostra projeto, origem e última atividade disponível.
- [ ] Integração GitHub de serviços e GitHub Copilot permanecem distinguíveis.
- [ ] Preferências antigas continuam carregando e o Claude mantém o comportamento anterior.

### Instalação, transporte e aprovações

- [ ] Prévia não altera configuração da CLI.
- [ ] Instalar, reinstalar e remover preservam hooks de terceiros, incluindo entradas adicionadas depois.
- [ ] Configuração inválida ou modificada desde a prévia interrompe a gravação.
- [ ] Configuração de usuário/projeto não provoca duplicação silenciosa de eventos gerenciados.
- [ ] Payload inválido, excessivo ou desconhecido é descartado com diagnóstico limitado.
- [ ] Named pipe e verificação do usuário continuam respeitando o isolamento local; validar permissões efetivas do transporte.
- [ ] App fechado, pausado ou com WebView indisponível preserva o fluxo nativo da CLI dentro do orçamento definido pelo adapter.
- [ ] Cliques atrasados, duplicados, de outra sessão ou anteriores ao reinício não produzem aprovação.
- [ ] Tentativa de decisão sem a janela de aprovação ativa/focada não autoriza a ferramenta.
- [ ] Para cada controle anunciado, uma sessão real comprova permitir, negar e devolver ao fluxo nativo.

### Estratégia de testes

1. Fixtures sanitizadas verificam normalização e formatos de resposta de cada adapter.
2. Testes em diretórios temporários verificam backup, merge, idempotência, conflito de prévia e remoção seletiva.
3. Testes de sessão verificam concorrência, IDs iguais em domínios distintos, turnos consecutivos e timers atrasados.
4. Testes de transporte verificam falhas, payloads, desconexões, expiração e reinício.
5. Smoke com CLIs reais registra versão, superfície, cenário, resultado e evidência.
6. Smoke visual Windows verifica ilha compacta/expandida, seleção, foco, bandeja, monitor/DPI, pausa e funcionamento das integrações atuais.

O `windows/package.json` atual não tem comando de testes do frontend. Definir o runner mínimo adequado na fase 1, justificando eventual dependência de desenvolvimento; não assumir que scripts do Agent Island existem no Coucou.

Os checks previstos partem dos scripts atuais: `npm run build`, `cargo fmt --all --check`, `cargo test --locked --workspace`, `cargo clippy --locked --workspace --all-targets -- -D warnings` e `npm run pack`, dentro de `windows`. Fazer o baseline antes de exigir ausência de novos avisos. Os comandos são planejamento de validação; não foram executados para este documento.

Registrar cada combinação como **aprovada**, **falhou** ou **não testada**. CLI ausente, sem autenticação ou sem emissão real de eventos permanece não testada. Um pacote que compila não prova integração real.

### Indicadores de sucesso locais

- Zero mistura de sessão nos cenários de concorrência definidos.
- Zero autorização decorrente de falha, silêncio ou expiração.
- Instalação/reinstalação/remoção aprovadas para os quatro formatos declarados.
- Meta inicial de atualização visual: p95 inferior a 1 segundo entre recebimento local e atualização da ilha, a medir em smoke com carga definida.
- Ilha ociosa sem novo polling frequente de processos/transcrições; medir CPU em condição fixa e comparar com o baseline.

Os indicadores são critérios propostos de validação local, sem introduzir telemetria ou afirmar desempenho já medido.

## 8. Riscos e decisões pendentes

| Questão | Decisão proposta | Quando resolver |
|---|---|---|
| O usuário utiliza CLI, desktop ou VS Code? | Planejar CLIs como primeira superfície; registrar as demais individualmente | Fase 0, antes de prometer cobertura. |
| O Codex desktop executa os hooks desejados nas sessões deste usuário? | Fazer prova real da superfície; app-server é alternativa para sessões por ele expostas/controladas, não prova de observação deste desktop | Investigação específica. |
| Há identidade confiável para múltiplas sessões em todos os eventos? | Validar ID nativo e domínio; não usar apenas o diretório | Fase 0/1. |
| Aprovações diferem por fornecedor e harness? | Declarar capacidades individualmente e manter fallback nativo | Antes de cada habilitação. |
| Instalações simultâneas do Agent Island e Coucou geram dois observadores? | Detectar/explicar sobreposição e evitar dois controles concorrentes de aprovação | Fase 0 e fluxo de instalação. |
| O frontend tem espaço para todas as sessões? | Agrupar por agente e listar sessões na expansão, com lista limitada/rolagem | Fase 5. |
| Hooks mudam entre versões? | Registrar versões qualificadas e recusar instalação em formato desconhecido | Todos os adapters. |
| Qual componente do Agent Island vale portar? | Escolher peças pequenas com testes e atribuição; manter stack Coucou | Fase 0. |

Nenhuma data de entrega está acordada. O maior risco de esforço está na qualificação das superfícies e nas aprovações, não em adicionar quatro ícones.

## 9. Chat controlado pelas CLIs: implementação autorizada

**Direção confirmada e implementação autorizada pelo usuário em 01/10/2026:** usar o próprio agente CLI como motor do chat do Coucou. A camada Rust/TypeScript foi implementada com Codex como opção inicial, adapters das quatro CLIs e seleção manual da API Anthropic legada. O funcionamento e as limitações estão em [CHAT-CLI-WINDOWS.md](CHAT-CLI-WINDOWS.md); a evidência por versão e cenário está em [VALIDACAO-CHAT-CLI-WINDOWS-2026-10-01.md](VALIDACAO-CHAT-CLI-WINDOWS-2026-10-01.md).

Fluxo proposto: chat da ilha -> backend Rust -> adapter do agente -> processo CLI local -> eventos estruturados de mensagem/ferramenta/aprovação -> chat da ilha. Cada conversa pertence a um agente, uma sessão e um diretório escolhido. A configuração e a autenticação efetivas da CLI permanecem sob controle da ferramenta; não copiar seus tokens para o frontend.

| Agente | Interface documentada a qualificar | Observação |
|---|---|---|
| Codex | [`codex app-server`, JSONL sobre stdio](https://learn.chatgpt.com/docs/app-server) | Oferece threads, turnos, streaming e aprovações. Confirmado no help da CLI local 0.159.2; está marcado experimental. Qualificar o schema dessa versão. |
| Claude Code | [Modo programático `-p` com stream-json](https://code.claude.com/docs/en/headless) e [Agent SDK para permissões](https://code.claude.com/docs/en/agent-sdk/permissions) | Qualificar continuidade, cancelamento e interação de ferramentas. O modo headless e o SDK têm diferenças; não assumir autorização automática para oferecer login de assinatura num produto distribuído. |
| Gemini CLI | [ACP](https://geminicli.com/docs/cli/acp-mode/) ou [headless com stream-json](https://geminicli.com/docs/cli/headless/) | ACP atende a uma UI interativa; detectar a flag compatível com a versão instalada. CLI ausente nesta execução. |
| GitHub Copilot CLI | [ACP nativo sobre stdio](https://docs.github.com/en/copilot/reference/copilot-cli-reference/acp-server) ou SDK oficial | ACP está em public preview e permite um cliente Rust sem sidecar SDK. CLI ausente nesta execução. |

Começar pelo Codex, que já está disponível localmente. A primeira entrega do chat deve incluir seletor de agente, diretório do projeto, conversa nova/retomada por ID, resposta incremental, cancelar turno e cartão de aprovação ligado à conversa correta. Alterações de arquivos e comandos precisam usar a política e as decisões documentadas do agente; não ativar bypass de permissões como solução de integração.

O Coucou controla suas próprias sessões. Retomar um histórico não equivale a anexar-se a um terminal interativo já aberto; impedir controle concorrente da mesma sessão. A nova camada bidirecional de chat é distinta do relay de observabilidade. Quando ambas receberem a mesma atividade, correlacionar os IDs e evitar dois cartões de aprovação para um único pedido.

Não assumir equivalência entre assinatura e chave de API: respeitar a modalidade de autenticação, limites e requisitos de integração de cada fornecedor. Para Codex, a [autenticação oficial](https://learn.chatgpt.com/docs/auth) distingue login ChatGPT e chave de API. Para Claude, conferir as regras de [integração via SDK](https://code.claude.com/docs/en/agent-sdk/overview) antes de oferecer autenticação de assinatura em distribuição a terceiros.

Critérios de aceite: histórico isolado por conversa, cwd correto, streaming sem interpretar a TUI/ANSI, interrupção confirmada pelo runtime, aprovações sem concessão implícita, ausência de processos órfãos, falta de CLI/login com erro claro e sessão real observada por agente/versão. Arquivos e contexto de janela precisam de envio consentido e capacidades qualificadas. Na implementação, Codex 0.159.2 passou em dois turnos reais de leitura com login ChatGPT, streaming e retomada por ID. Claude 2.1.87 passou somente no handshake real, sem prompt. Ferramentas/aprovações reais, Gemini/Copilot e a janela nativa permanecem com as limitações descritas no relatório.

## 10. Entrega e fronteiras deste plano

O planejamento inicial adicionou somente este documento. A implementação autorizada posteriormente está descrita em `docs/MULTIAGENTE-WINDOWS.md`, com evidência em `docs/VALIDACAO-MULTIAGENTE-WINDOWS-2026-10-01.md`. As etapas de aprovação adicional e expansão continuam separadas. Nenhum hook global ou chave foi aplicado pela implementação.

Antes de implementar, preservar as alterações locais existentes de `windows/package-lock.json` e os executáveis não rastreados na raiz. Usar mudanças pequenas e revisáveis, com uma branch `codex/` quando a execução for iniciada.

O resultado da implementação deve incluir código, testes relevantes, matriz de suporte, manual de instalação/remoção e pacote candidato Windows. Publicação é um passo de entrega distinto. O workflow atual tem `PUBLISH: 'false'`; o plano não muda esse estado. Preservar as atribuições de licença dos componentes eventualmente reaproveitados.

## 11. Referências

- Código Coucou: `windows/src/core/state.ts`, `windows/src/island/hooks.ts`, `windows/src-tauri/src/pipe.rs`, `windows/hook/src/main.rs`, `windows/src-tauri/src/hooks.rs`.
- Regras do projeto: `CLAUDE.md`, `CONTRIBUTING.md`, `windows/README.md`.
- [Claude Code — Hooks](https://code.claude.com/docs/en/hooks).
- [Codex — Hooks](https://developers.openai.com/en-US/docs/hooks).
- [Codex — App-server](https://developers.openai.com/codex/app-server).
- [Gemini CLI — Hooks reference](https://geminicli.com/docs/hooks/reference/).
- [GitHub Copilot — Hooks reference](https://docs.github.com/en/copilot/reference/hooks-reference).
- [VS Code — Configure agent hooks, Preview](https://code.visualstudio.com/docs/agent-customization/hooks).
- Referências locais do Agent Island: `crates/core/src/adapters.rs`, `crates/core/src/protocol.rs`, `crates/hook-client/src/installer.rs`, `docs/integrations.md`, `docs/integrations-copilot.md`, `docs/manual-smoke.md` e `THIRD_PARTY_NOTICES.md`.

Revalidar documentação oficial, versão da CLI e superfície no momento da implementação. A matriz de suporte final deve registrar evidência real, e não apenas links ou compatibilidade inferida.
