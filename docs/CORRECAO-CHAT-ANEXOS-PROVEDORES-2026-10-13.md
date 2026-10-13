# Correção do chat, anexos e provedores — 13/10/2026

## Objetivo

Reparar as regressões da mescla e permitir que o agente pessoal use o provedor escolhido no Coucou. Preservar hooks, credenciais, histórico e confirmação de acessos locais.

## Diagnóstico

- As abas de texto reutilizavam dimensões de abas de ícones. O wrapper do chat e o painel pessoal não ocupavam a altura do painel: sobrava uma área preta e o conteúdo podia ser cortado.
- O chat oficial era somente conversação. Selecionar um modelo não conectava as ferramentas locais; o usuário recebia respostas genéricas de ausência de acesso.
- O agente local iniciava no Hermes, embora esse runtime pudesse estar ausente. A disponibilidade do agente ficava associada a um serviço opcional.
- O drop oficial alimentava `State.droppedFile`, mas não os anexos da conversa pessoal. PDFs sem suporte no formato de mensagem do provedor podiam chegar apenas com nome, sem conteúdo.
- As configurações não diferenciavam com clareza hooks, provedor do chat, motor do agente e serviço FreeLLMAPI.

## Funcionamento corrigido

`Agente local → Coucou · provedor escolhido` usa Anthropic ou a API OpenAI compatível dos provedores configurados: OpenAI, Google, OpenRouter, Ollama, LM Studio e servidor customizado. O seletor e o cofre são os mesmos do chat oficial. Hermes/FreeLLMAPI, Codex e Claude continuam opcionais. Um endpoint FreeLLMAPI compatível pode ser conectado como servidor customizado; não é obrigatório instalar Hermes para usar o agente API.

O agente API reutiliza sessões, histórico, contexto pessoal, cancelamento e cartões de aprovação. Não inicia subprocessos de CLI. O broker Rust existente valida os argumentos, caminhos, anexos selecionados e permissões. Cada operação API exige confirmação única, com identificação do provedor/modelo de destino; não herda concessões ao trocar de modelo. O turno tem limite de seis etapas, oito ferramentas, 90 segundos por requisição de rede e oito minutos totais. IDs de chamada repetidos e ferramentas fora do catálogo são bloqueados. Não há repetição automática nem fallback de fornecedor.

O modo local abre por padrão no Windows; a escolha explícita da aba Chat é respeitada durante essa abertura do aplicativo. O Chat simples permanece disponível. Nenhum modelo é consultado ao selecionar um modo ou motor.

O arquivo arrastado só entra na lista de anexos pessoais após a cópia no inbox terminar. A importação é feita uma vez por caminho; falhas permanecem na interface e não são repetidas automaticamente. A seleção não envia uma pergunta ao modelo. Ao enviar, o serviço de documentos prepara texto, referências e cobertura. O agente pode ler outros trechos de anexos selecionados com `coucou_read_attachment`. O chat simples também passa pela extração Rust para documentos não visuais; erro de leitura é explícito. PDFs digitalizados sem texto e imagens sem OCR têm limitações visíveis.

## Verificação do arquivo real

O arquivo `Metodologia_Desenvolvimento_Evolutivo_Solucoes_Digitais_V2.pdf`, encontrado no inbox do Coucou, foi lido pelo subprocesso de extração do próprio aplicativo: 652.533 bytes, 26 páginas lidas de 26, 26.249 caracteres extraídos, nenhuma página vazia, sem truncamento de extração e sem chamadas de rede. Isso qualifica leitura local, não comprova que um fornecedor remoto produziu um resumo. Trechos enviados por turno continuam sujeitos ao orçamento de contexto.

## Atualizações oficiais

O checkout incorpora `origin/main` em `58f2889` (merge local `0ea4722`), consultado em 13/10/2026. As mudanças Windows desde a mescla anterior eram de catálogo/traduções. As mudanças Android incluem widgets e o lançamento 0.2.0.

- E-mail: o módulo `NotchBuddy/Sources/App/MailController.swift` lê Apple Mail por automação macOS. Não há equivalente de caixa de entrada Windows nessa base. A pill Resend é uma integração distinta e não implementa leitura da caixa de entrada ou envio de anexos pelo drop.
- Celular: existe o vínculo Android, com backend Windows em `windows/src-tauri/src/phone_link` e seção própria de configuração. O app oficial requer Android 11+ e Coucou desktop 0.4.0+. Não houve instalação no telefone nem pareamento físico nesta validação.

Fontes: [releases oficiais](https://github.com/Louis-CFM/coucou/releases), [Android oficial](https://github.com/Louis-CFM/coucou/blob/main/android/README.md), [Apple Mail oficial](https://github.com/Louis-CFM/coucou/blob/main/NotchBuddy/Sources/App/MailController.swift).

## Validação e entrega

Suíte completa: 593 testes Rust aprovados (548 aplicativo, 12 protocolo, 30 relay e três do executável relay), zero falhas, cinco opcionais ignorados. Frontend: 499 aprovados, zero falhas, um opcional ignorado. TypeScript e Vite aprovados. A última build 0.4.0-agent.2 passou novamente nos quatro testes selecionados por `api`, incluindo autorização nativa simulada e consulta real de processos. O teste de regressão ficou vermelho ao restaurar o Hermes como motor obrigatório e voltou a passar após restaurar a correção. `cargo fmt --all --check` e whitespace aprovados.

Layout verificado em Edge com HTML de fixture que monta os componentes e CSS reais: painel 304 px, área ativa 272 px, card pessoal 212 px, campo de mensagem dentro do card e sem overflow horizontal. O seletor de modelos usa o espaço disponível sem cortar o editor. Não é captura da janela Tauri.

Versão de entrega: `0.4.0-agent.2`, versão MSI `0.4.0.2`. Empacotamento e instalação: em andamento. Clippy estrito tinha avisos no upstream na entrega anterior; a aprovação dos testes não elimina essa pendência. Evidências locais em `windows/target/chat-repair-*`, `windows/target/document-real-file-probe/evidence.json` e `windows/output/playwright/`. O preview visual usa dados de teste e não substitui clique nativo de aprovação, chamada a provedor real ou pareamento Android.
