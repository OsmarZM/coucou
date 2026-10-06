# Integração do Coucou oficial e atualização Windows 0.1.3

## Objetivo

Incorporar o repositório oficial sem substituir a conversa pessoal, a memória, os documentos e as fronteiras de autorização construídos neste fork. Eliminar a confusão entre fonte atualizada e aplicativo antigo aberto.

## Origem e preservação

- Origem: `https://github.com/Louis-CFM/coucou.git`, branch `main`, commit `83708fe` — Prepare Coucou 0.1.9 (#261).
- Base pessoal: `9c90c3e`, branch `codex/multiagent-windows`.
- Foram incorporados 90 commits do oficial posteriores à base comum.
- Ponto de recuperação: `codex/backup-before-upstream-20261006`, apontando para `9c90c3e`.
- Os três executáveis antigos da raiz foram preservados, sem inclusão no Git.
- Os dados de perfil, documentos e credenciais não fazem parte do merge.

## Resolução dos conflitos

As mudanças de macOS/iPhone, recursos e documentação entraram no histórico do fork. No Windows, foram combinadas as abstrações de plataforma, o transporte do hook e o empacotamento MSI com a implementação pessoal existente. O lockfile preserva as versões já selecionadas, acrescentando as dependências de plataforma necessárias.

O protocolo normalizado de agentes, a identidade das sessões, o instalador reversível de hooks e as permissões locais prevalecem sobre os fluxos legados. Gemini e Copilot continuam bloqueados para chat até a qualificação do isolamento de hooks/MCP. Nenhuma configuração global de fornecedor foi alterada pelo merge e nenhuma mensagem foi enviada a outros chats.

O código de macOS/iPhone e Linux foi importado; isso não comprova build ou funcionamento dessas plataformas neste fork. As extensões pessoais continuam específicas de Windows e requerem adaptação e qualificação próprias antes de distribuir um pacote Linux deste fork.

## Aplicativo e interface

A janela observada estava executando `D:\coucou\coucou.exe`, uma cópia antiga. A interface daquele binário ainda expõe “Codex CLI”, seleção de conversa e pasta na tela principal e textos em inglês. Atualizar o Git não recompila nem substitui esse arquivo.

A versão Windows 0.1.3 usa os personagens Codex, Claude, Gemini e Copilot. Pasta, retomada e seleção de canais separados ficam recolhidas em Avançado. O rodapé identifica a versão real. `Iniciar-Coucou.ps1` abre o executável compilado atual, mantendo os arquivos antigos intactos e recusando encerrar outras cópias automaticamente.

## Situação real do Hermes

O Hermes Agent completo não está instalado, incorporado ou executado por nenhum personagem. A camada de memória do Coucou é uma implementação própria em Rust/SQLite, inspirada no Hermes: histórico persistente, preferências declaradas, busca, memórias e procedimentos versionados com revisão.

Não há aprendizado recorrente com modelo, treinamento de pesos ou coordenador Hermes distribuindo tarefas entre as quatro CLIs. Monitorar agentes e selecionar um personagem são funções distintas de delegação. A integração futura do runtime precisa preservar as confirmações de arquivo e o consentimento individual para mensagens a sessões externas; não pode herdar ferramentas, MCP ou automações sem qualificação.

Referências oficiais: [memória](https://hermes-agent.nousresearch.com/docs/user-guide/features/memory), [delegação](https://hermes-agent.nousresearch.com/docs/guides/delegation-patterns).

## Validação

- Frontend: 103 testes passaram; TypeScript e build Vite de produção passaram.
- Rust: 142 testes passaram; três testes que exigem modelo real permaneceram ignorados.
- `cargo fmt --all --check` e Clippy do workspace com `-D warnings` passaram.
- As configurações existentes foram copiadas para um backup local antes de trocar o aplicativo. Não havia banco pessoal existente nesse perfil; nenhum registro foi excluído.

- `npm run pack` passou e gerou os instaladores NSIS e MSI 0.1.3 em `windows/release/`.
- O worker de documentos do executável de produção passou nos 10 casos, sem iniciar GUI ou modelo. O teste de prazo valida a limpeza do harness, não a qualificação completa do Job de produção.
- A cópia antiga ociosa foi encerrada após conferir seu caminho e a ausência de processos CLI associados. O lançador abriu `windows/target/release/coucou.exe`.
- A janela nativa confirmou os quatro personagens, textos em português, Avançado recolhido, aviso de permissão por operação e rodapé `Coucou 0.1.3`. Nenhuma mensagem foi enviada a um modelo ou outro chat para essa conferência.

| Artefato | SHA-256 |
| --- | --- |
| `Coucou-Windows-0.1.3-setup.exe` | `392A86A3F16FB1F729E8F7BD33E3816D61C813986719A027CA78F045A2988F6D` |
| `Coucou-Windows-0.1.3.msi` | `DE064D2AE368358A4EA82EF1BC54E374A5E5DB4AF0D2E35A0C080B25A673C620` |
| `target/release/coucou.exe` | `F32D4373189BDD9BD35545CDF8AD1F8A7BDFB95EB61B365C5C74F0BBF45ADB4B` |

Os instaladores foram gerados, mas não instalados nesta validação. Testes com modelo real, instalação/upgrade de MSI/NSIS e qualificação de DPI, múltiplos monitores, despertar por hover e arrastar arquivos são verificações separadas. A publicação do código no fork não publica esses binários em GitHub Releases.
