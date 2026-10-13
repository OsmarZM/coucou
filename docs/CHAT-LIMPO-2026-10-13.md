# Chat limpo — 0.4.0-agent.4

## Objetivo

Deixar a conversa principal com mensagens, anexos e campo de envio, conforme solicitado pelo usuário.

## Funcionamento

A apresentação compacta do chat principal não monta o cabeçalho Automático, o botão Contexto, o painel de projetos/sessões, a explicação de compartilhamento de anexos nem o rodapé sobre armazenamento. Os anexos aparecem junto ao campo de mensagem, depois da conversa. O resumo é solicitado escrevendo uma mensagem ao agente; o botão Ler e resumir não aparece nessa apresentação.

Avisos informativos de sessões externas e detalhes de disponibilidade não ocupam o chat. Erros reais, importação em andamento, cancelamento e confirmações de autorização continuam visíveis quando necessários. A conversa vazia não tem texto introdutório. A conexão continua usando a configuração salva.

O componente reutilizável mantém os controles de projeto na apresentação não compacta, sem modificar adapters CLI, permissões, credenciais ou armazenamento. O chat principal usa explicitamente a apresentação compacta.

## Validação

- Regressão reproduzida: os testes de ausência do cabeçalho falharam antes da alteração.
- 505 testes do frontend aprovados, nenhuma falha, um teste opt-in ignorado.
- Testes verificam ausência de cabeçalho/controles introdutórios, preservação do editor, anexos e área de autorização, ausência de troca de conexão e ausência de envio automático.
- TypeScript e build Vite de produção aprovados.
- Preview Edge com os componentes/CSS reais em `windows/output/playwright/chat-clean.png`, usando a fixture isolada `windows/dev/clean-preview.html`. Conferido visualmente: conversa vazia sem textos superiores e editor na parte inferior. Nenhum fornecedor ou arquivo real foi acionado.

Esses testes usam fixtures de DOM e não comprovam conversa com fornecedor remoto nem clique de autorização na janela nativa.

## Entrega local

NSIS e MSI gerados em `windows/release/`. Instalador NSIS aplicado com sucesso (código 0) na instalação existente `D:/Nova pasta/Coucou`. Executável instalado identificado como `0.4.0-agent.4` e reaberto. O hash de `settings.json` permaneceu igual ao backup antes da abertura.

Backup: `C:/Users/FortaTech/AppData/Local/Coucou-update-backups/before-clean-chat-agent4-20261013-180536`.

- NSIS SHA256: `863177e39bd020efbd833767e173e6ec05166b11f1cf1ae9d71fdb2b4c32a501`.
- MSI SHA256: `5efb59fc85db977326fbd33777a980af32d862d8327552b08c351b1bae953d5f`.

Os executáveis anteriores na raiz do repositório foram preservados. Nenhuma publicação no GitHub foi feita.
