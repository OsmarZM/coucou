# ADR-001: Hermes como supervisor e FreeLLMAPI como conexão opcional de modelos

**Status:** Accepted; implementação Windows 0.1.4 em qualificação.

**Date:** 2026-10-06

**Deciders:** usuário/Product Owner e responsável técnico do Coucou.

**Plano:** [Hermes, CLIs e APIs gratuitas](PLANO-HERMES-ORQUESTRADOR-E-FREELLMAPI.md).

## Context

O Coucou Windows 0.1.3 já tem conversa pessoal, adapters CLI, documentos, memória SQLite e controles de autorização. A memória atual é própria, inspirada no Hermes. O usuário quer o runtime real coordenando CLIs, um personagem pessoal nomeável com APIs gratuitas opcionais, seleção pela guia principal e anexos que funcionem sem bloqueio silencioso.

Há restrições concretas: preservar dados/trabalho existente, impedir mensagens não autorizadas e duplicadas em outros chats, não expor terminal irrestrito e não apresentar a capacidade anunciada do FreeLLMAPI como crédito garantido. Monitoramento, chat, conexão de modelo e delegação são capacidades distintas.

## Decision

Integrar Hermes como processo gerenciado por ACP em perfil exclusivo do Coucou. O Coucou preserva interface, história comum, documentos, supervisão de processos e decisão final de permissões. Hermes recupera contexto, formula propostas de memória/procedimentos e pede operações através de uma ponte tipada para o broker Rust, que usa os adapters existentes.

O escritor persistente continua sendo o serviço Rust/SQLite do Coucou. Preferências explícitas de baixo risco são capturadas na conversa; `memory_propose` e `skill_propose` recebem propostas do Hermes e aplicam a política de origem/curadoria existente. O snapshot aprovado é espelhado nos arquivos do perfil privado, sem sincronização bidirecional. As ferramentas nativas de memória/skills do Hermes permanecem desativadas porque contornariam essa política e poderiam introduzir execução fora dos controles do broker. O runtime real está integrado, mas o aprendizado nativo completo do Hermes não está habilitado nesta adaptação.

FreeLLMAPI é um serviço local opcional e autenticado, restrito a loopback. Serve modelos configurados para Hermes, sem converter as CLIs em clientes do roteador por padrão. Cada usuário escolhe quais contas/destinos habilitar. Um registro de autostart inicia infraestrutura após login apenas quando habilitado, sem iniciar tarefas de agentes.

A guia principal é o único local cotidiano para escolher personagens. Nome não fica acoplado ao modelo. Na primeira entrega, o personagem Hermes usa o supervisor; Codex e Claude mantêm canais diretos compatíveis com o contexto compartilhado. Converter os personagens especialistas em preferências de executor do supervisor e permitir aparência arbitrária são evoluções posteriores. Nenhuma escolha autoriza ferramentas ou retomada externa.

Corrigir o fluxo de anexos antes de colocar mais componentes no caminho. Integrar Codex e Claude por fases; não liberar Gemini/Copilot só porque Hermes ou uma API correspondente está disponível.

## Options Considered

| Opção | Complexidade | Vantagem | Custo e consequência |
| --- | --- | --- | --- |
| A — manter apenas memória própria | Baixa | Menor distribuição; preserva stack | Não entrega runtime/delegação Hermes solicitado |
| B — runtime Hermes separado com broker Coucou | Média/alta | Reutiliza agente real e controles existentes; componentes atualizáveis separadamente | Exige qualificar ACP, ferramentas, memória e empacotamento de runtimes |
| C — copiar/mesclar todo o código Hermes e FreeLLMAPI no Coucou | Alta | Código disponível em um checkout | Acopla releases/dependências e amplia manutenção; não elimina necessidade de isolamento |
| D — Hermes inicia CLIs via terminal irrestrito | Baixa no protótipo | Rápido para demonstração | Contorna aprovação, deduplicação e controle de sessões; inadequado para esta entrega |

Recomendação: B. Incorporar componentes versionados necessários, com atribuição/licença, sem fundir as aplicações completas nem expor shell ao modelo.

## Trade-off Analysis

Hermes precisa de um modelo configurado para coordenar o trabalho, independentemente do login das CLIs. A revisão fixada exige contexto mínimo de 64.000 tokens; o usuário informa a capacidade documentada do modelo, até 1.000.000 nesta integração. O valor configurado não é uma medição nem aumenta o limite do provedor. O modo gratuito reduz dependência de uma API paga, mas acrescenta contas/chaves, cotas e variabilidade. Não substitui os requisitos de autorização e capacidade do executor.

Uma conversa visível contínua não implica uma sessão técnica eterna. Sessões internas, recuperação e snapshots mantêm custo/contexto limitados. A memória do perfil integrado tem um escritor persistente Rust; o Hermes produz propostas sem receber acesso irrestrito à escrita. História, recibos e permissões continuam sob responsabilidade do Coucou.

Preferências diretas e correções de baixo risco podem alimentar a memória automaticamente; documentos e resultados de executores não promovem instruções autoritativas sozinhos. A conexão gratuita é opcional para delegação. Sua preferência de inicialização é independente da inicialização preexistente do Coucou e não a remove quando o usuário desativa o roteador.

Transportes padronizados reduzem acoplamento, mas ACP e MCP não são sandboxes. A prova do conjunto efetivo de ferramentas e das recusas no backend é condição de habilitação.

## Consequences

- O agente pessoal pode ter nome próprio e trocar modelos sem trocar de identidade; personalização de avatar permanece fora da primeira entrega.
- O usuário seleciona personagens pela ilha; Hermes coordena seus auxiliares na timeline comum. Os canais especialistas diretos permanecem disponíveis.
- Novos runtimes exigem revisão de licença, integridade, atualização, empacotamento e suporte Windows.
- A delegação tem limites, orçamento, cancelamento e recibos próprios. Não há retomada automática de resultado incerto.
- Quotas, tokens e resets são exibidos com origem/qualidade; capacidade desconhecida não vira saldo fictício.
- O modo CLI existente permanece caminho de compatibilidade enquanto o supervisor estiver sem conexão ou não qualificado.
- O perfil integrado expõe somente oito operações MCP do Coucou. O conjunto efetivo é conferido antes do prompt; terminal, código, plugins/MCP herdados, mensagens e agendamento são bloqueados.
- Há até dois auxiliares novos, somente leitura, por turno Hermes, executados sequencialmente na primeira implementação. Cada objetivo exige autorização única; auxiliares não delegam novamente nem retomam chats externos.
- Modelos Groq/Cerebras, login real do autostart e instalação/upgrade exigem qualificação própria; instalar/configurar infraestrutura não prova inferência.

## Action Items

1. [ ] Qualificar drop/seletor nativos após as correções de progresso, erro, prazo e cancelamento.
2. [x] Concentrar seleção de personagens na guia principal e remover lista/barra duplicada do chat.
3. [ ] Qualificar Hermes ACP com modelo real, retomada e cancelamento no perfil restrito.
4. [ ] Qualificar FreeLLMAPI opcional com conta/modelo autorizados de cada provedor.
5. [ ] Qualificar delegação real a Codex/Claude após integração dos contratos de aprovação/deduplicação.
6. [x] Definir escrita única, propostas tipadas e snapshots, preservando Contexto e memória existente.
7. [ ] Qualificar login real de autostart, uso, instalação e rollback em outra máquina.

Resultados e limites de cada gate estão no [relatório da entrega](VALIDACAO-HERMES-FREELLMAPI-WINDOWS-2026-10-06.md). Itens implementados e itens homologados com runtime/provedor real são estados separados.

Nenhuma decisão deste ADR instala componentes, concede acesso permanente ou autoriza mensagens a chats externos.
