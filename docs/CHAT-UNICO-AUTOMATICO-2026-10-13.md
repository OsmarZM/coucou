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

Empacotamento, instalação e backup serão registrados na conclusão. Logs em `windows/target/chat-auto-*` e `windows/target/unified-chat-*`. Versão de aplicativo 0.4.0-agent.3; versão MSI 0.4.0.3.
