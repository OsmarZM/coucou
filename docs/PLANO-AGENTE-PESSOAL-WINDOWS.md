# Plano: Coucou como agente pessoal no Windows

- **Data:** 02/10/2026
- **Status:** implementação local em qualificação; consultar a matriz de evidências em [VALIDACAO-AGENTE-PESSOAL-WINDOWS-2026-10-02.md](VALIDACAO-AGENTE-PESSOAL-WINDOWS-2026-10-02.md)
- **Projeto:** D:\coucou
- **Direção confirmada:** preservar a interface do Coucou e adotar o padrão de aprendizado do Hermes Agent da Nous Research
- **Base local:** branch codex/multiagent-windows, com alterações anteriores preservadas

## 1. Objetivo

Transformar o chat do Coucou em um assistente pessoal que conversa sem exigir uma pasta de projeto, acessa arquivos mediante autorização adequada, lê anexos, mantém contexto entre sessões e aprende preferências e procedimentos. Preservar a ilha escura, o Mochi, as animações e os cards compactos dos screenshots.

O resultado deve permitir:

- Perguntar “quantos arquivos tenho em Downloads?” sem preencher um caminho técnico.
- Autorizar a operação necessária no momento em que ela for proposta.
- Arrastar documentos, escolher o agente e conversar sobre o conteúdo.
- Reabrir o Coucou e continuar com preferências, projetos e procedimentos relevantes.
- Passar o cursor no topo central e encontrar a ilha; manter o compacto visível por padrão.
- Ver atividade, sessões, subagentes, processos, tokens e cotas com fonte e horário.
- Usar toda a interface do produto em português brasileiro.

Aprendizado aqui significa persistir e recuperar conhecimento útil, melhorar procedimentos e aplicar correções. Esta evolução não exige treinar os pesos de Codex, Claude, Gemini ou Copilot.

## 2. Diagnóstico confirmado

| Área | Estado atual no código | Mudança necessária |
|---|---|---|
| Chat CLI | Há adapters de Codex, Claude, Gemini e Copilot; a entrada exige uma pasta absoluta existente e envia texto | Modo pessoal com workspace interno, contexto e anexos tipados |
| Identidade da conversa | Agente, pasta e permissão são vinculados à conversa | Preservar retomada; separar diretório de execução e alvos autorizados |
| Histórico | Mensagens CLI ficam na memória do frontend; metadados limitados ficam no armazenamento local | Histórico persistente do Coucou e recuperação por relevância |
| Drop | Copia o arquivo para o inbox; o fluxo atual de anexos está ligado ao chat Anthropic API | Anexos por conversa para os adapters CLI |
| Retenção | Inbox tem limpeza por idade de sete dias | Retenção por referência; anexos em uso não podem desaparecer |
| Ilha | FSM fecha expandido após 15 segundos fora e oculta compacto após 60 segundos; hover oculto abre compacto | Política persistida de visibilidade, fixação e proteção de interação |
| Português | Textos ingleses estão distribuídos pelas views, Settings e upload | Catálogo central pt-BR |
| Atividade | Sanitização ampla pode omitir toda a mensagem, inclusive por URL ou sinal de igualdade | Resumos estruturados e proteção de valores sensíveis |
| Métricas | Os transports ainda não encaminham vários campos de uso para a interface | Serviço de uso e adapters específicos por fornecedor |

Ponteiros principais: windows/src/core/agent-chat.ts, windows/src/views/chat.ts, windows/src/island/fsm.ts, windows/src/island/island.ts, windows/src-tauri/src/island.rs, windows/src-tauri/src/files.rs e windows/src-tauri/src/agent_chat/.

Os screenshots confirmam a experiência apresentada. A causa de uma falha específica de hover, OLE ou monitor exige reprodução na janela Tauri nativa.

## 3. Como incorporar o Hermes

O Hermes distingue perfil do usuário, memória compacta, procedimentos reutilizáveis e busca no histórico. Sua documentação descreve persistência entre sessões e carregamento de procedimentos quando relevantes. Essas ideias serão a referência funcional do Coucou. [Memória oficial](https://hermes-agent.nousresearch.com/docs/user-guide/features/memory), [skills oficiais](https://hermes-agent.nousresearch.com/docs/user-guide/features/skills).

### Decisão recomendada

Manter o Coucou como interface, armazenamento comum e ponto de autorização. Implementar a camada pessoal na stack atual, reaproveitando os contratos e mecanismos úteis do Hermes após revisão. Preservar os adapters das quatro CLIs e seus logins existentes.

| Abordagem | Vantagem | Custo e condição |
|---|---|---|
| Adaptar memória, skills e recuperação ao backend Rust do Coucou | Uma camada compartilhada entre as quatro CLIs; distribuição Windows menor | Portar conceitos e testar comportamento; manter registro de origem quando houver código reaproveitado |
| Integrar Hermes como agente adicional por ACP ou processo gerenciado | Reutiliza o runtime Python e suas ferramentas | Exige distribuição, atualização, supervisão e permissões de um runtime adicional; provar o contrato ACP e os logins utilizados |
| Incorporar todo o runtime Hermes ao aplicativo | Acesso ao conjunto maior de funcionalidades | Aumenta dependências e superfície operacional; escolher somente após demonstrar vantagem sobre a integração modular |

A arquitetura oficial do Hermes tem um core Python, armazenamento de sessões e um servidor ACP. Ter suporte a modelos no Hermes não prova que ele funciona como substituto das quatro CLIs já instaladas. A integração de runtime entra como opção qualificada, mantendo a escolha do usuário. [Arquitetura oficial](https://hermes-agent.nousresearch.com/docs/developer-guide/architecture).

A licença principal é MIT. Código copiado ou adaptado deve manter os avisos exigidos; dependências e arquivos incorporados terão revisão própria. Antes de qualquer importação, fixar commit de origem, listar módulos e adicionar atribuição. [Licença oficial](https://github.com/NousResearch/hermes-agent/blob/main/LICENSE).

### Referência de código para a implementação

Revisão pesquisada: **c67b0a1dc4a08fe5437f5cb6e1391cbb864bcbf6**, no repositório NousResearch/hermes-agent. Fixar essa revisão como referência inicial e reavaliar mudanças antes de atualizar.

| Componente Hermes | Mecanismo observado | Aplicação proposta no Coucou |
|---|---|---|
| [memory_tool_store.py](https://github.com/NousResearch/hermes-agent/blob/c67b0a1dc4a08fe5437f5cb6e1391cbb864bcbf6/tools/memory_tool_store.py) | Limites, deduplicação e persistência | Escrita transacional e orçamento explícito |
| [session_search_tool.py](https://github.com/NousResearch/hermes-agent/blob/c67b0a1dc4a08fe5437f5cb6e1391cbb864bcbf6/tools/session_search_tool.py) | Busca de mensagens reais sem chamada ao modelo, com limites de retorno | Recuperação local com cobertura indicada |
| [skill_manager_tool.py](https://github.com/NousResearch/hermes-agent/blob/c67b0a1dc4a08fe5437f5cb6e1391cbb864bcbf6/tools/skill_manager_tool.py) | Criação e atualização de procedimentos | Skills com versões, evidência e diff |
| [background_review.py](https://github.com/NousResearch/hermes-agent/blob/c67b0a1dc4a08fe5437f5cb6e1391cbb864bcbf6/agent/background_review.py) | Revisão com modelo e ferramentas restritas no despacho | Processo opcional, cancelável e com orçamento |
| [skill_manager_guards.py](https://github.com/NousResearch/hermes-agent/blob/c67b0a1dc4a08fe5437f5cb6e1391cbb864bcbf6/tools/skill_manager_guards.py) | Fronteiras para manutenção de skills sob gestão do curador | Separar procedimentos pessoais, de projeto e importados |
| [write_approval.py](https://github.com/NousResearch/hermes-agent/blob/c67b0a1dc4a08fe5437f5cb6e1391cbb864bcbf6/tools/write_approval.py) | Propostas pendentes e revisão | Gate Rust que não aplica gravação em falha |

Reaproveitar exige adaptar garantias, não apenas copiar funções. Na revisão, invalidar proposta cujo registro de origem mudou; preservar procedimentos escritos pelo usuário; confirmar gravação antes de apresentar sucesso. Não usar prazos de envelhecimento ou uma configuração permissiva de fallback como requisitos automáticos do Coucou.

### Ciclo de aprendizado proposto

1. Usar preferências confirmadas e recuperar histórico e procedimentos relevantes.
2. Executar a tarefa respeitando permissões e registrar resultado observado.
3. Identificar correções do usuário, passos que funcionaram e erros resolvidos.
4. Criar ou atualizar candidatos de memória e procedimentos, com evidência de origem.
5. Consolidar duplicatas, resolver conflitos e versionar alterações.
6. Reutilizar o conhecimento em outra tarefa e registrar se ainda funcionou.

A conversa alimenta o contexto continuamente, sem botão para ativar aprendizado. Nesta implementação não há chamadas adicionais a modelos para curadoria nem um agente fazendo chamadas contínuas em segundo plano. Candidatos são produzidos durante o turno solicitado pelo usuário e passam pela política de origem, risco e revisão.

## 4. Modo pessoal e permissões por necessidade

### Experiência

O chat abre em **Modo pessoal**, com uma timeline contínua e escolha do fornecedor pelo personagem animado. Não há seletores principais de agente ou conversa. Histórico, preferências e anexos pertencem ao contexto pessoal `personal-main`; IDs e permissões de cada CLI continuam separados internamente. “Projeto” e retomada de sessão ficam em controles avançados. O histórico de um projeto ou de um chat externo não é importado automaticamente para esse contexto.

Sem projeto, cada conversa recebe um workspace interno em %LOCALAPPDATA%\Coucou\conversations\<id>\workspace. A memória, o banco de dados e as credenciais ficam fora desse workspace. Ele satisfaz a necessidade de cwd da CLI e não autoriza explorar o computador.

Downloads, Documentos e Área de Trabalho serão resolvidos pelas Known Folders do Windows, respeitando OneDrive e redirecionamentos. Quando houver ambiguidade, o agente apresenta opções concretas, em vez de pedir que o usuário digite um caminho completo. [Known Folders da Microsoft](https://learn.microsoft.com/en-us/windows/win32/shell/known-folders).

### Exemplo de fluxo

1. Usuário: “Quantos arquivos tenho em Downloads?”
2. Coucou resolve a localização da pasta, sem enumerar seu conteúdo.
3. Se ainda faltar concessão, mostra: “Posso contar os arquivos desta pasta? A ação não lerá o conteúdo dos documentos.” O cartão exibe o caminho real e se inclui subpastas/arquivos ocultos.
4. Usuário permite uma vez ou durante a conversa.
5. O backend executa uma operação restrita de contagem e retorna quantidade, critério, momento da consulta e eventuais itens inacessíveis.
6. Uma ação posterior para abrir um documento exige a capacidade de leitura correspondente.

Na ausência de instrução específica, contar arquivos diretamente na pasta, sem recursão, e declarar a inclusão de ocultos. Subpastas são contadas separadamente. Pastas protegidas, atalhos e links não devem produzir uma contagem completa fictícia.

### Contrato proposto de autorização

| Capacidade | O que autoriza | O que exige nova decisão |
|---|---|---|
| Listar/contar | Nomes ou contagem em pasta e profundidade delimitadas | Ler conteúdos ou ampliar a busca |
| Ler | Arquivo ou conjunto explícito | Escrita, execução ou acesso a outra pasta |
| Criar/alterar | Destino e alteração mostrados | Outros arquivos ou efeitos adicionais |
| Executar | Comando, argumentos, diretório e efeitos | Comando diferente ou elevação |
| Enviar externamente | Serviço, destino e conteúdo definidos | Outro destinatário, publicação ou finalidade |

Cada concessão vincula agente, conversa, turno, operação, alvo canônico, validade e escopo. Opções iniciais: **Permitir uma vez**, **Permitir nesta conversa** e **Negar**. A lista de concessões permite revogação. Concessões permanentes por pasta podem entrar posteriormente com seleção explícita.

Um pedido direto inequívoco e um arquivo arrastado podem constituir autorização para a leitura necessária. Não repetir perguntas já respondidas para cada trecho do mesmo arquivo. Acessos novos, alvos ambíguos e efeitos adicionais precisam de decisão própria.

Negação, expiração, fechamento do aplicativo ou falha não viram autorização. Aprender uma preferência ou reutilizar um procedimento também não amplia permissões.

### Gate técnico obrigatório

O sandbox atual “somente leitura” não intercepta todas as leituras. Um texto no prompt pedindo consentimento não estabelece a fronteira de segurança.

O serviço de permissões do Coucou deve validar as ferramentas mediadas. Para CLIs com ferramentas nativas, qualificar sandbox e políticas efetivas; bloquear ou restringir caminhos que contornem o serviço. No Codex, avaliar profiles com read/write/deny e concessões do protocolo; essa superfície tem diferenças de maturidade e aplicação no Windows. [Permissões oficiais do Codex](https://learn.chatgpt.com/docs/permissions).

Antes de anunciar “acesso por alvo”:

- Provar leitura negada fora dos alvos, leitura aprovada no alvo e escrita negada.
- Testar caminhos canônicos, UNC, prefixos estendidos, junctions, symlinks e troca de alvo durante a operação.
- Verificar ferramentas de shell, filesystem, MCP e rede; não presumir que um controle cobre todas.
- Confirmar a política efetiva retornada pela CLI e as limitações do sandbox Windows.
- Usar configuração por sessão/processo e preservar configurações globais.
- Marcar agentes sem essa fronteira como capacidade não suportada, oferecendo anexos preparados ou modo projeto explícito.

Primeiro qualificar o Codex. Claude, Gemini, Copilot e um eventual Hermes terão seus próprios critérios.

## 5. Memória, contexto e procedimentos

### Quatro camadas compartilhadas

| Camada | Exemplo | Recuperação |
|---|---|---|
| Perfil | “Responder em português e apresentar primeiro a solução prática” | Resumo compacto no início da conversa |
| Contexto e projetos | Nome do projeto, localização, stack e relações confirmadas | Conforme assunto e escopo |
| Procedimentos/skills | Forma aprovada de pesquisar, validar uma API ou preparar uma entrega | Catálogo resumido; corpo carregado quando necessário |
| Histórico | Conversas e evidências de tarefas anteriores | Busca por texto e referências |

Isso permite que uma preferência aprendida no Codex seja relevante numa conversa com Claude, sem misturar históricos nativos ou credenciais. Procedimentos de um projeto não devem vazar para outro por coincidência de termos.

### Política recomendada

Conforme a orientação do usuário de 02/10/2026, o contexto é aprendido pela própria conversa, com persistência automática. Declarações literais de baixo risco podem ser consolidadas com origem registrada. A curadoria automática usa categorias fechadas de preferências de apresentação e fatos profissionais; não presume intenção a partir de um trecho parcial. Observações ambíguas, novos procedimentos e orientações de execução entram como candidatos até revisão. Histórico, memória e procedimentos nunca concedem acesso a arquivos, execução ou envio a outros chats.

Procedimentos já aprovados podem receber propostas de melhoria após falhas ou correções. A revisão mostra o diff, a origem e o impacto; uma versão anterior permanece recuperável. Não transformar um único erro em regra permanente.

Cada registro proposto inclui identificador, tipo, conteúdo, escopo, origem, evidência, estado, versão e datas de criação/última confirmação. Confiança é uma avaliação auxiliar; a autoridade vem da origem e da confirmação. Correções explícitas prevalecem sobre inferências.

Na interface, **Contexto** permite pesquisar, corrigir, aprovar, rejeitar, exportar e esquecer memórias, procedimentos e histórico. Não é um botão de ativação do aprendizado. Preferências de idioma e formato preservam o sentido da declaração, incluindo negações; fatos sobre o mundo continuam exigindo verificação de atualidade.

Segredos, chaves, cookies e credenciais não entram em memória, skills ou índice. Documentos e resultados de ferramentas são dados externos: instruções contidas neles não alteram o sistema nem autorizam ações. Scanners de conteúdo ajudam, mas não substituem isolamento e controle de autoridade.

### Persistência

Recomendação: SQLite no backend Rust, com migrações, transações e busca FTS. Justifica uma dependência nova por histórico consultável, consistência e coordenação entre agentes concorrentes. Medir impacto no instalador e avaliar licença antes de escolher a biblioteca.

O backend é o único responsável pela escrita. Evitar quatro CLIs editando os mesmos arquivos de memória. Skills podem ser importadas/exportadas como diretórios com SKILL.md e referências; conteúdo importado é revisado antes de ativação.

Nos registros que exigem revisão humana, falha na configuração, validação ou armazenamento deixa a proposta pendente ou rejeitada; nunca aplica a alteração por fallback. Confirmar “aprendi” somente depois de uma gravação efetiva, com identificador e versão recuperáveis.

Manter contexto com orçamento limitado: perfil compacto, memórias relevantes, procedimentos necessários e trechos dos anexos. Não reenviar toda a biblioteca em cada mensagem. Congelar a base da sessão quando isso beneficiar cache; correções novas usam atualização controlada ou nova conversa.

**Esquecer** remove o registro, índices e sua recuperação futura no Coucou. Mostrar separadamente as opções de apagar histórico e cópias de anexos. Não prometer apagar conteúdo já enviado ou armazenado pelas CLIs/provedores.

## 6. Documentos e drag-and-drop

### Fluxo único

Arrastar ou clicar em **Anexar arquivos** → registrar anexos no contexto pessoal → preparar/validar → mostrar nome, tipo, tamanho e estado → enviar pergunta ao personagem escolhido → responder com referências ao conteúdo. Trocar de personagem preserva a fila; o turno recebe somente os anexos selecionados naquele envio.

O drop não apaga o histórico e não dispara uma conversa sem intenção de envio. O card informa o agente de destino. Leitura local do arquivo e envio ao fornecedor são etapas visíveis, seguindo a preferência de consentimento configurada.

### Cobertura por incremento

| Incremento | Formatos | Comportamento |
|---|---|---|
| Primeiro | TXT, Markdown, código, JSON e CSV | Leitura local limitada; trechos com linha/origem |
| Segundo | PDF textual e DOCX | Extração em worker; referências de página ou seção |
| Terceiro | Imagens e PDF escaneado | Visão nativa qualificada ou OCR identificado; sem prometer suporte universal |
| Posterior | XLSX e outros formatos de escritório | Extração tipada por planilha, sem executar macros |

Limites iniciais propostos para qualificação: até 10 anexos, 20 MiB por arquivo, 50 MiB por lote, 200 páginas por PDF e 30 segundos por extração. São limites de produto ajustáveis após testes; não representam limites do fornecedor. O texto efetivamente enviado tem orçamento separado.

DocumentService valida tipo real e tamanho, copia para caminho com ID seguro, registra hash, extrai em worker cancelável e prepara trechos referenciáveis. Não executar código, macros ou comandos do documento. Dependências de extração serão justificadas por formato, licença, distribuição Windows e contenção de entrada.

Preservar os originais e oferecer remoção do anexo. Recusar arquivos protegidos ou inválidos com erro em português. Detectar truncamento, ausência de texto e páginas não lidas; resposta deve indicar a cobertura. Evitar tratar CSV como conhecimento completo quando apenas uma amostra foi lida.

A retenção passa a acompanhar referências de conversa e escolha do usuário. Itens ainda referenciados não são apagados por uma varredura genérica de sete dias.

## 7. Ilha, idioma e atividade

### Visibilidade

Recomendação inicial: **Sempre visível**, mantendo a ilha compacta no topo. Fechar o painel recolhe para o compacto. **Ocultar automaticamente** fica como opção configurável.

Adicionar fixação do usuário no header. Separar essa preferência das retenções temporárias de aprovação, drag, foco e edição. Um cartão respondido não pode desfazer a fixação do usuário.

Quando explicitamente oculta, passar o cursor pela faixa superior central revela a ilha e permite abrir o último painel. Qualificar a expansão por hover com atraso curto, sem roubar foco; clique dá foco ao chat. Disponibilizar também tray e atalho configurável.

Não fechar por timer enquanto houver foco de edição, rascunho, seleção de texto, drag/extração ou aprovação pendente. Preservar rascunhos ao trocar abas, recolher e reabrir.

O mecanismo de hover precisa continuar alcançável quando o cursor poll principal está estacionado. Reavaliar eventos nativos de entrada/display e geometria, mantendo baixo consumo quando o usuário não interage. Não aumentar polling global para 60 Hz permanentemente.

### Português

Criar catálogo pt-BR para abas, configurações, upload, botões, estados, erros conhecidos, permissões e acessibilidade. Exemplos: **Conversa**, **Nova conversa**, **Agentes**, **Arraste seus arquivos aqui**, **Aguardando autorização**, **Renova às**, **Indisponível**.

Nomes de agentes/modelos, comandos e caminhos permanecem corretos. O contexto solicita respostas em português, sem reescrever silenciosamente respostas livres do fornecedor.

### Diagnósticos

Exibir atividade estruturada, como “Contando arquivos · concluído” ou “PowerShell · em execução”. Dados sensíveis são redigidos por campo/valor; resumos seguros continuam legíveis. Detalhes omitidos têm aviso em português, recolhido, sem repetir placeholders no corpo da conversa.

Revisar a regra que omite toda mensagem por qualquer sinal de igualdade ou URL com casos benignos e casos reais de credenciais. Uma melhoria visual nunca deve despejar stderr ou comandos brutos sem proteção. Aprovações continuam exibindo detalhes inspecionáveis da ação.

## 8. Agentes, processos, tokens e cotas

### Contagens sem duplicação

Nos ícones, badge compacto de sessões ocupadas e indicação de ação pendente. Ao expandir, mostrar:

- **Sessões principais:** identidade nativa por fornecedor; somente sessões em atividade.
- **Subagentes em execução:** filhos com identidade e eventos explícitos.
- **Ferramentas em execução:** chamadas vinculadas ao turno.
- **Processos locais:** PIDs e início de processo atribuídos à árvore gerenciada.

Sessão aguardando autorização aparece como ocupada e com esse estado. Uma cadeia npm → node → rg não vira três agentes. Um subagente não vira nova sessão principal.

Mesclar monitoramento por hook e eventos do chat somente quando houver vínculo confiável de identidade. Sessões externas terão cobertura parcial; não atribuir processos por coincidência de nome. Falta de eventos ou perda de heartbeat vira estado desconhecido/desconectado, sem deixar contagens antigas “em execução”.

### Exemplo visual ilustrativo

    Codex    2 sessões · 1 subagente
    3 ferramentas · 5 processos locais
    Conversa: 28.400 tokens observados
    Janela de 5 h: 62% disponíveis · renova às 18:40
    Semanal: 74% disponíveis · renova em 05/10, 10:00
    Atualizado há 30 s

Os números acima são apenas exemplo de interface. Barras de conta ficam no detalhe do fornecedor; não repetir o saldo como se cada sessão tivesse cota própria.

### Semântica de uso

**Tokens da conversa**, **ocupação do contexto**, **cota da conta** e **custo monetário** são métricas diferentes. Capturar a fonte mais confiável por sessão. Deduplicar IDs, snapshots acumulados, retomadas e consumo de subagentes; nunca somar um total acumulado a cada notificação.

Mostrar janela de cinco horas e semanal apenas quando forem reportadas para aquela conta. A duração e a unidade vêm do fornecedor. “Renova em 2 h” é tempo até reset; não significa duas horas de trabalho disponíveis.

Calcular percentual restante a partir de uso observado, quando a métrica permitir. Não converter tokens em número de mensagens nem em percentual de assinatura. Reset vencido sem atualização vira **Aguardando atualização**. Campos ausentes ou nulos precisam de semântica específica do adapter; um bucket explicitamente removido não deve continuar válido no cache.

### Matriz de capacidade verificada para planejamento

| Fornecedor | Tokens/contexto | Cota e renovação | Implementação proposta |
|---|---|---|---|
| Codex | thread/tokenUsage/updated; account/usage/read conforme versão | account/rateLimits/read e notificações; buckets por conta | Primeiro adapter, apoiado no schema local 0.159.2; qualificar auth e resposta reais |
| Claude | Eventos de uso de stream-json/SDK | Eventos e statusline em versões/planos compatíveis | Captura passiva; instalada 2.1.87 não comprova novos campos de statusline |
| Gemini | Resposta ACP pode reportar tokens; usage_update indica contexto | Quotas dependem do login; getter de conta ACP estável não confirmado | Qualificar versão; mostrar saldo indisponível quando o canal não o expõe |
| Copilot | Uso de sessão por ACP ou SDK/RPC qualificado | account.getQuota no SDK/RPC; não é método ACP padrão | Coletor separado ou comandos informativos anunciados pelo servidor; respeitar unidade/periodicidade da conta |

Codex expõe duração e reset dos buckets; não fixar primary como cinco horas e secondary como semana. A disponibilidade de uso da conta depende da autenticação. [App-server oficial](https://learn.chatgpt.com/docs/app-server).

Claude documenta campos opcionais de rate limits na statusline em versões compatíveis; a instalação atual precisa de qualificação. Alterar statusline global requer prévia e preservação da configuração existente. [Statusline oficial](https://code.claude.com/docs/en/statusline).

Gemini tem regras diferentes conforme Google login, API key ou Vertex; tokens e contexto ACP não provam saldo da assinatura. Não enviar /stats ao ACP sem contrato anunciado, pois pode virar prompt ao modelo. [Quotas oficiais](https://geminicli.com/docs/resources/quota-and-pricing/), [comandos ACP no código oficial](https://github.com/google-gemini/gemini-cli/blob/main/packages/cli/src/acp/acpCommandHandler.ts).

Copilot tem interfaces oficiais de uso e quota fora do ACP padrão. Usar apenas comandos informativos reconhecidos pelo servidor e qualificar previews/RPCs antes de exibir números. [Uso e billing](https://docs.github.com/en/copilot/how-tos/copilot-sdk/features/usage-and-billing), [ACP oficial](https://docs.github.com/en/copilot/reference/copilot-cli-reference/acp-server).

Contrato normalizado proposto: fornecedor, conta pseudonimizada, versão, origem, escopo, unidade, valor, data de captura e qualidade (**observado**, **estimado**, **parcial**, **indisponível**). Não guardar credenciais junto às métricas.

Preferir eventos e atualizar em término de turno, abertura do detalhe e atualização manual, com cache/backoff. Contagem regressiva é local. Não iniciar turnos de modelo para sondar quota; não comprar créditos, consumir resets ou trocar para API paga automaticamente.

## 9. Organização técnica proposta

    Interface atual do Coucou
        -> ConversationService
        -> ContextBuilder
             -> MemoryService / SkillsService / SessionSearch
             -> DocumentService
        -> AgentChatManager -> adapter CLI selecionado
             -> CapabilityBroker -> ação local autorizada
             -> UsageService -> tokens e cotas
        -> ActivityRegistry -> sessões, filhos, ferramentas e processos

Módulos propostos, sem obrigar uma refatoração geral:

| Responsabilidade | Local sugerido |
|---|---|
| Modo pessoal e contexto | src-tauri/src/personal/ e core/personal-context.ts |
| Política e concessões | src-tauri/src/capabilities/ |
| Extração e anexos | src-tauri/src/documents/ e core/attachments.ts |
| Memória e procedimentos | src-tauri/src/memory/, src-tauri/src/skills/ |
| Contabilidade | src-tauri/src/usage/ e core/usage.ts |
| Traduções | src/i18n/pt-BR.ts |
| Controle visual | Ajustes incrementais na FSM e island.rs existentes |

Nomes de módulos e entidades são propostas deste plano, não APIs já existentes. Reaproveitar transport, cancelamento, aprovação e Windows Job atuais. O banco do Coucou não substitui o histórico nativo das CLIs.

## 10. Fases e critérios de conclusão

| Fase | Entrega | Evidência para encerrar |
|---|---|---|
| F0 — contratos e spikes | Fixar origem Hermes; definir memória/skills, identidade e fronteira de permissões; spike Codex no Windows | Decisão de integração documentada; acesso fora do alvo efetivamente negado |
| F1 — experiência previsível | Português, compacto persistente, fixação, hover e diagnósticos legíveis | Uso nativo sem perda de rascunho nem fechamento durante interação; strings pt-BR |
| F2 — modo pessoal | Projeto opcional, Known Folders e operações restritas de listar/contar/ler com concessões | Caso Downloads completo; negativas e revogação verificadas |
| F3 — documentos | Fila por conversa, texto/código; depois PDF/DOCX e visão/OCR | Conteúdo correto, cobertura indicada, cancelamento e retenção; anexar preserva histórico |
| F4 — aprendizado Hermes | Histórico pesquisável, perfil, memória e skills versionadas; revisão e “O que aprendi” | Preferência atravessa reinício e troca de agente; correção e esquecimento funcionam |
| F5 — painel de uso | Contagens e Codex primeiro; outros fornecedores conforme contrato | Sem duplicação; reset e conta corretos; ausências e dados antigos identificados |
| F6 — homologação e pacote | Matriz nativa por CLI/versão; build e instalador | Relatório aprovado/falhou/não testado por cenário e artefato verificável |

F1 pode avançar durante os spikes de F0. Codex usage de F5 pode ser entregue cedo em paralelo, após estabilizar o contrato de métricas. A camada de persistência é fundação de F3/F4; o ciclo de aprendizado vem depois que permissões e anexos estiverem claros.

**Primeiro incremento recomendado:** F0/F1 e o caso “contar Downloads” do Codex em F2. Depois, anexos, memória e painel ampliado. Isso resolve os problemas atuais e cria a fronteira necessária para evoluir os procedimentos pessoais.

## 11. Validação

### Casos obrigatórios

- Contagem direta e recursiva, ocultos, pasta vazia, OneDrive/redirecionamento e itens inacessíveis.
- Negar leitura, revogar acesso, expirar concessão e tentar usar concessão em outra conversa.
- Conteúdo malicioso num PDF/skill não gera ferramenta nem permissão adicional.
- Arquivo com nome repetido, arquivo grande, extração inválida e cancelamento não corrompem conversa.
- Memória declarada, hipótese incorreta, correção posterior e conflitos entre projetos.
- Duas CLIs concorrentes não sobrescrevem memórias; restart e migração preservam registros.
- Excluir memória impede recuperação no Coucou; UI explica alcance da exclusão.
- Uma sessão com subagentes e processos filhos mantém contagens e tokens sem duplicação.
- Resume, snapshot repetido e troca de conta não somam consumo antigo nem misturam cotas.
- Campos de quota ausentes, bucket removido, reset expirado e CLI offline são apresentados corretamente.

### Windows nativo

Homologar DPI 100%, 125%, 150% e 200%; monitores com escalas diferentes e coordenadas negativas; alteração de resolução; desconexão de monitor com ilha oculta; hover no topo; click-through; foco; OLE drop; tray e atalho. Registrar consumo da ilha ociosa.

Preview no navegador valida layout. Fixtures e testes de protocolo validam contratos. Sessões reais validam a integração daquele agente, versão e autenticação. Esses resultados ficam separados no relatório.

Executar verificações proporcionais às fases: testes de política, persistência e reducers; build frontend; Rust fmt/test/clippy; build desktop. Instalação/upgrade do pacote é um cenário próprio, não consequência automática do build.

## 12. Limites do levantamento inicial

Este documento se baseia no código local, screenshots, schemas do Codex e fontes oficiais consultadas em 01/10/2026. Não foram feitas chamadas a modelos, leitura de cotas reais, instalação do Hermes, atualização de CLIs ou mudanças de configuração global.

Gemini e Copilot não estavam disponíveis no PATH nas verificações locais anteriores. Suporte de código e documentação não constitui homologação dessas CLIs. Versões e capacidades serão verificadas novamente na implementação.

A autorização para reaproveitar/integrar Hermes foi registrada, e a revisão de referência está indicada neste documento. O levantamento inicial preservou o código existente e não dependeu de mesclar históricos Git de projetos diferentes.

## 13. Decisões da implementação em 02/10/2026

- Memória, histórico e procedimentos implementados no backend Rust/SQLite, inspirados nos mecanismos do Hermes. Nenhum runtime Python ou código do Hermes foi incorporado ao pacote.
- Aprendizado e persistência fazem parte da conversa desde a abertura, conforme a última orientação do usuário. A migração atualiza os flags antigos sem apagar histórico. A interface usa personagens e uma timeline pessoal contínua; sessões e IDs nativos permanecem separados. Não há adoção automática de chats externos ou histórico de projeto.
- Declarações de baixo risco e sustentadas integralmente pela mensagem original podem ser consolidadas automaticamente. Ambiguidades e orientações de execução permanecem pendentes; procedimentos têm origem, escopo, versão e diff obrigatório para revisão. Importação/restauração não pula aprovação.
- O Codex pessoal usa `environments: []` e ferramentas dinâmicas restritas para contar/listar/ler arquivos, propor aprendizados, carregar procedimentos aprovados e ler trechos adicionais dos anexos escolhidos. Uma versão que não confirme essas restrições é recusada antes de um turno.
- Claude pode conversar no modo pessoal com ferramentas nativas desativadas; a contagem e leitura sob permissão do Coucou estão implementadas inicialmente para Codex. O chat Gemini/Copilot fica bloqueado nos modos pessoal e projeto até a qualificação do isolamento de hooks e MCP. A instalação, autenticação e observação por hooks permanecem disponíveis separadamente; o adapter ACP foi preservado.
- Anexos pessoais são compartilhados entre os personagens no contexto contínuo, com integridade por hash e referências de linha/página/parágrafo. Texto, código, CSV, JSON, PDF textual e DOCX sem macros são suportados. Imagens, OCR, PDF somente digitalizado, DOC legado e outros formatos mostram indisponibilidade; não há leitura fictícia.
- Visibilidade permanente do compacto é o padrão; ocultação automática exige escolha. Foco, edição, anexos, execução e aprovação seguram a expansão. Preferência de fixação do usuário é independente das retenções temporárias.
- Uso reportado pelos protocolos é separado em tokens, contexto e cota da conta. Snapshots cumulativos substituem anteriores. Identidade de conta, versão/fonte e horário ficam vinculados; quota ausente não vira zero e reset vencido aguarda nova informação.
- A regra de comunicação entre chats é obrigatória por envio: origem no backend, prévia integral, autorização única, bloqueio de duplicatas/concorrência e ausência de retentativas automáticas. Ver [controle de mensagens entre chats](CONTROLE-DE-MENSAGENS-ENTRE-CHATS.md).

O documento de validação distingue implementação, testes, chamadas reais com CLI, preview no navegador, pacote produzido e os cenários de Windows ainda não homologados. O build não instala o aplicativo nem altera configurações globais de hooks.
