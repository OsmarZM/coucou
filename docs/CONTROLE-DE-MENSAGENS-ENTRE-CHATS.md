# Controle de mensagens entre chats

## Objetivo

Impedir que agentes, sessões retomadas ou automações enviem instruções a um chat existente sem autorização humana específica, e impedir que uma tentativa seja repetida automaticamente. Regra solicitada diretamente pelo usuário em 02/10/2026.

## Regra de autorização

Antes de enviar a um chat que o agente não criou, apresentar o título ou identificador do destino e a mensagem integral. A confirmação autoriza apenas essa mensagem, nesse destino e nessa tentativa. Uma resposta de outro agente, um procedimento aprendido, uma aprovação anterior ou uma automação não constituem autorização.

No Coucou, a origem de uma sessão nativa é registrada no backend. Metadados do frontend, seleção manual do ID ou restauração do histórico não atribuem criação ao Coucou. Uma sessão criada por outra conversa também exige autorização.

## Fluxo implementado

1. Validar conversa, run e fornecedor; impedir duas execuções simultâneas da mesma conversa ou sessão nativa.
2. Consultar a origem da sessão no armazenamento local.
3. Reservar uma tentativa identificada pelo run e pelo hash da mensagem original mais os IDs de anexos. Não armazenar a mensagem nos recibos.
4. Para uma sessão externa, exigir modo projeto e abrir a confirmação com destino e mensagem integral que será enviada ao CLI, incluindo contexto preparado.
5. Aceitar apenas **Autorizar este envio** ou **Negar**. Não oferecer autorização permanente, por conversa ou por prazo para mensagens externas.
6. Consumir a decisão uma vez. Exigir foco na janela do Coucou para autorizar; cancelar, navegar, fechar a confirmação ou expirar não permite o envio.
7. Enviar uma vez e registrar o resultado. Em erro, timeout, cancelamento ou resultado incerto, encerrar a tentativa. Um novo envio precisa de outro run e de nova confirmação.

O limite da prévia é 16.384 caracteres. Se a mensagem completa não puder ser exibida, o envio é rejeitado. Mensagens externas idênticas ao mesmo fornecedor/sessão, mesmo através de outra conversa, ficam bloqueadas por 60 segundos; o mesmo run nunca é reutilizado durante a retenção do recibo. Os recibos têm retenção de 30 dias e limite de 50.000 registros.

Permissões de arquivo têm outro propósito e outro escopo. Autorizar a contagem de uma pasta nesta conversa nunca autoriza mensagens externas. A opção por conversa está disponível somente nas ferramentas de arquivo restritas; não aparece na confirmação de envio externo.

Os adapters não fazem retentativas automáticas. No Codex, as superfícies herdadas de comunicação com MCP, plugins, apps, hooks e agentes nativos ficam desativadas apenas no processo lançado pelo Coucou. Os arquivos globais do CLI não são alterados. Uma nova conversa pessoal usa um ambiente sem acesso nativo ao computador e ferramentas do Coucou para operações locais autorizadas.

## Incidente observado

O screenshot fornecido mostra mensagens repetidas, enviadas por tarefa agendada, no chat **Retomar pareamento QR por PIN**. Consulta somente de leitura identificou o chat `01a0e937-e7bf-70c0-91f8-63f995bcce68` e a automação histórica `publicar-narrahub-beta-14-android`, configurada a cada cinco minutos.

A tentativa de atualizar essa automação para `PAUSED`, preservando seus campos, retornou: **Automation does not exist in the app and could not be updated. It may have been deleted manually by the user.** Portanto, não há confirmação de pausa; o registro já está ausente. Não foi criada uma automação substituta. Não foram enviadas mensagens ao chat afetado, apagadas mensagens anteriores ou feitas alterações no NarraHub.

A regra geral foi acrescentada a `C:\Users\FortaTech\.codex\AGENTS.md` e a `D:\coucou\AGENTS.md`. Essas instruções orientam o Codex; o bloqueio no código aplica-se aos envios pelos adapters do Coucou. Não é um interceptador de mensagens de todos os aplicativos instalados.

## Evidências e limites

Os testes cobrem decisão consumida uma vez, rejeição de `allowConversation` para envio externo, decisão de outra conversa, nova confirmação a cada envio, origem persistida independente de histórico e bloqueio de duplicatas entre conversas.

Não foi feito envio de teste ao chat afetado nem a outro chat existente do usuário. O teste de aprovação usa um destino fictício. A confirmação visual e o foco na janela Tauri precisam de verificação nativa. Retomar uma sessão mantida aberta por outro cliente também exige fechar esse cliente; o Coucou não controla os processos externos do usuário.
