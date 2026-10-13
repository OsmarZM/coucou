# Chat único em Automático — 0.4.0-agent.3

## Objetivo

Remover a separação entre Chat e Agente local e centralizar a escolha de conexão nas configurações, conforme solicitado pelo usuário.

## Mudanças

- A janela principal tem uma única conversa, com um único editor de mensagem. Não há abas de Chat/Agente local.
- Os seletores de Coucou, Hermes/FreeLLMAPI, Codex, Claude, provedor e modelo foram retirados do chat.
- O chat mostra Automático e usa o provedor/modelo já salvos no Coucou. Não há seleção automática entre contas, fallback novo ou envio de mensagens ao abrir a interface.
- Configurações → Conexão do chat automático permite escolher provedor, consultar modelos e salvar o modelo. Consultar exige clique explícito; alterar os campos não grava preferências até Salvar conexão do chat. Credenciais permanecem nas seções e no cofre existentes.
- Anexos, contexto/histórico, leitura de documentos, permissões e ferramentas continuam no mesmo fluxo. Os adapters CLI e o runtime Hermes permanecem no backend; retirar seus botões não apaga configurações ou sessões.
- Plataformas sem o runtime pessoal mantêm a superfície de conversa com provedores. O frontend não ativa funções Windows em Linux.

## Validação

504 testes de frontend aprovados, zero falhas, um opt-in ignorado. TypeScript aprovado. O teste que exige chat único ficou vermelho antes da mudança; reintroduzir os seletores antigos também falhou, e restaurar a implementação Automático voltou a passar. Os novos testes verificam rascunho de configuração sem gravação, salvamento explícito sem alterar monitor/telefone, recusa de modelo inválido antes do IPC e ausência de consulta de modelos na abertura do chat.

Preview Edge com componentes e CSS reais em `windows/output/playwright/chat-auto.png`: sem abas e seletores, editor e Contexto preservados. É uma fixture visual, não uma captura nativa nem comprovação de fornecedor remoto. O backend funcional não foi alterado; apenas versões Rust/Tauri foram alinhadas à entrega. O build de produção recompila e verifica a integração.

Empacotamento NSIS e MSI concluído. A instalação em `D:/Nova pasta/Coucou/coucou.exe` foi atualizada para 0.4.0-agent.3 e reaberta; processo respondendo. O executável foi comparado integralmente à build gerada (diferença somente da marca NSIS do Tauri). O hash de settings.json ficou idêntico ao backup antes da abertura. Logs em `windows/target/chat-auto-*` e `windows/target/unified-chat-*`. Versão de aplicativo 0.4.0-agent.3; versão MSI 0.4.0.3.

Backup antes da instalação: `C:\Users\FortaTech\AppData\Local\Coucou-update-backups\before-auto-chat-agent3-20261013-175453`.

NSIS SHA256: `a45ff7ac0bb992119ec2d5f03c6a9f15824b64d1940e1b59c093a2b1e6d89cc9`. MSI SHA256: `469d2d488342ef92e47032c75ea318b6c319e406c96d9ce77be7e715083b3426`.
