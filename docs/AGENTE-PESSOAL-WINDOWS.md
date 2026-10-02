# Agente pessoal do Coucou — Windows 0.1.2

## Objetivo

Conversar no Coucou sem escolher uma pasta ou uma conversa a cada tarefa. O personagem escolhido determina a CLI que responde. A conversa mantém contexto local, preferências e documentos; as permissões continuam explícitas para cada operação que precisar delas.

## Conversa e personagens

Abra o chat e escolha um personagem animado habilitado: Codex ou Claude. A troca não envia uma mensagem sozinha. No modo pessoal, os canais compatíveis usam a mesma timeline e os mesmos anexos. O rascunho continua disponível ao trocar de personagem. Os personagens Gemini e Copilot continuam presentes para identificação e monitoramento, com aviso de que o chat está indisponível nesta versão.

O aplicativo mantém um ID próprio para cada canal e os IDs de sessão de cada fornecedor. Isso permite retomar uma sessão local sem misturar as execuções. O histórico enviado em um novo turno tem orçamento limitado; partes antigas ou extensas podem ser recortadas ou omitidas, com indicação de recorte. Não há promessa de enviar todo o histórico ao modelo em cada mensagem.

Codex suporta as ferramentas pessoais mediadas pelo Coucou. Claude pode conversar no modo pessoal com ferramentas nativas desativadas. O chat Gemini e Copilot está indisponível em todos os modos nesta 0.1.2 até a qualificação do isolamento de hooks e MCP; escolher uma pasta de projeto não habilita o envio.

A análise do Gemini 0.62.0 confirmou que hooks globais/de projeto e servidores MCP herdados podem continuar ativos mesmo em modo de planejamento, executando comandos fora das confirmações do Coucou. O Copilot não estava instalado no ambiente de validação e esse isolamento ainda não foi qualificado. As CLIs podem ser instaladas, autenticadas e monitoradas; o fluxo manual de instalação dos hooks do Coucou continua separado, com prévia e revisão. Os adaptadores de chat são preservados para qualificação futura. Veja as referências oficiais de [hooks do Gemini](https://geminicli.com/docs/hooks/) e [hooks do Copilot](https://docs.github.com/en/copilot/reference/hooks-reference).

Projeto e retomada manual de IDs dos fornecedores habilitados ficam em **Avançado**. Conversas externas e projetos não entram automaticamente na timeline pessoal. Enviar a uma sessão que não foi criada pelo Coucou exige uma confirmação para aquele envio, com destino e mensagem completa.

## Contexto aprendido

Não é necessário ativar o aprendizado. As mensagens são persistidas localmente e recuperadas como contexto informativo. Preferências de apresentação e fatos profissionais declarados literalmente, de baixo risco, podem ser consolidados com sua origem. Por exemplo: “Prefiro respostas em português”. A avaliação automática preserva a declaração inteira e não transforma uma negação em preferência oposta.

O painel **Contexto** mostra **Memórias**, **Procedimentos** e **Histórico**. Use-o para consultar, corrigir, revisar, exportar ou esquecer registros. Procedimentos e mudanças de processos passam por proposta, diff e revisão. Lembrar de um processo não autoriza executá-lo.

Não há treinamento dos pesos do modelo nem chamadas recorrentes de aprendizado em segundo plano. O contexto é composto durante os turnos solicitados. Credenciais detectadas são recusadas no armazenamento pessoal e na preparação dos documentos.

## Arquivos e permissões

No Codex pessoal, pergunte “Quantos arquivos tenho em Downloads?”. O Coucou resolve a pasta conhecida do Windows e apresenta a operação, o alvo real e a profundidade antes de contar. Não é necessário preencher a pasta de projeto para essa tarefa.

Uma concessão fica ligada ao agente, canal, ferramenta, alvo e validade. Autorizar contagem não libera leitura do conteúdo, escrita ou outra pasta. Trocar de personagem não transfere a concessão. É possível negar e revogar permissões.

Arraste arquivos para a ilha ou use **Anexar arquivos**. A fila apresenta estado de preparação e cobertura. O envio usa os anexos selecionados e preserva a conversa. PDFs textuais, DOCX sem macros, texto, código, CSV e JSON têm leitura local com referências de página, parágrafo ou linha. Imagens, OCR, PDFs só digitalizados e DOC legado não têm leitura nesta versão.

Os documentos são copiados para o armazenamento local, verificados por hash e processados em worker separado com limites. Conteúdo de um documento é tratado como dado; não pode autorizar ferramentas ou mensagens externas. Um arquivo sem cobertura integral não é apresentado como totalmente lido.

## Visibilidade e métricas

A ilha compacta permanece visível por padrão. Fixação e ocultação são escolhas do usuário; foco, rascunho, anexos, execução e confirmação pendente mantêm a expansão. A revelação pelo cursor no topo central está implementada, mas a homologação em DPI e múltiplos monitores ainda precisa de validação na janela nativa.

Os personagens e cards mostram a atividade reportada: sessões, subagentes observáveis, ferramentas/processos, tokens e cota da conta quando a CLI a expõe. Tokens, ocupação do contexto e cota são medidas distintas. Os períodos de cinco horas e semana aparecem somente quando reportados pela conta; indisponibilidade não é tratada como zero. Um reset vencido aguarda nova informação.

## Esquecimento e entrega

Esquecer no Coucou remove o registro local e sua recuperação futura. Não apaga sessões ou logs mantidos pelo fornecedor, mensagens do Codex desktop ou os documentos originais. Memórias, procedimentos, histórico e cópias de anexos possuem controles próprios.

Veja a evidência e as limitações em [VALIDACAO-AGENTE-PESSOAL-WINDOWS-2026-10-02.md](VALIDACAO-AGENTE-PESSOAL-WINDOWS-2026-10-02.md). A regra de autorização única por envio e o incidente anterior estão documentados em [CONTROLE-DE-MENSAGENS-ENTRE-CHATS.md](CONTROLE-DE-MENSAGENS-ENTRE-CHATS.md).
