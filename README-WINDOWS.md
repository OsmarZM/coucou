# Coucou pessoal no Windows — instalação passo a passo

## Objetivo e escopo

Instalar o Coucou 0.1.2, preparar o login das CLIs e usar os personagens animados. O chat está habilitado para Codex CLI e Claude Code; Gemini CLI e GitHub Copilot CLI podem ser instalados e monitorados, mas o chat desses dois fornecedores fica indisponível nesta versão até a qualificação do isolamento de hooks e MCP. A conversa pessoal mantém histórico e anexos compartilhados entre os canais compatíveis. Não é necessário ativar aprendizado ou escolher uma pasta para começar.

O repositório desta implementação é [OsmarZM/coucou — branch codex/multiagent-windows](https://github.com/OsmarZM/coucou/tree/codex/multiagent-windows). Escolha livremente a pasta do projeto em seu computador; não é necessário ter uma unidade `D:`. Esta entrega disponibiliza o código e o guia pelo branch; o instalador deve ser gerado pelos comandos da seção 4. O workflow Windows mantém `PUBLISH=false`, com a publicação automática de instalador pausada. Consulte a [validação](docs/VALIDACAO-AGENTE-PESSOAL-WINDOWS-2026-10-02.md) para os resultados reais e limites.

## 1. Escolher a forma de instalar

| Situação | Caminho |
|---|---|
| Você recebeu o instalador 0.1.2 | Instale as CLIs desejadas, seguindo a seção 3, e o aplicativo, na seção 5 |
| Você tem o código e quer gerar o instalador | Prepare o ambiente na seção 2 e execute a seção 4 |
| Você quer desenvolver e testar a janela Tauri | Siga as seções 2 a 4 e use `npm run tauri dev` |

O aplicativo instalado não precisa de Rust, Visual Studio, Docker ou banco de dados externo. As ferramentas de compilação são necessárias apenas para gerar o aplicativo a partir do código. Os fornecedores continuam exigindo sua própria autenticação e plano/acesso correspondente.

## 2. Preparar o Windows para compilar

Use Windows x64 e PowerShell. O projeto Windows foi preparado com Tauri 2; os cenários de DPI, múltiplos monitores e upgrade ainda precisam de homologação na janela nativa.

1. Instale [Git for Windows](https://git-scm.com/downloads/win).
2. Instale [Node.js](https://nodejs.org/en/download), versão 22 ou superior. Prefira uma versão LTS. Node 22 também atende ao requisito de instalação do Copilot CLI por npm.
3. Instale [PowerShell 7](https://learn.microsoft.com/powershell/scripting/install/installing-powershell-on-windows). Use uma janela nova após instalar ferramentas ou alterar o PATH.
4. Instale as [Build Tools do Visual Studio](https://visualstudio.microsoft.com/visual-cpp-build-tools/). No instalador, selecione **Desenvolvimento para desktop com C++**, incluindo MSVC e Windows SDK.
5. Instale [Rust pelo rustup](https://rustup.rs/), com a toolchain MSVC do Windows. Não escolha GNU para este build.
6. Garanta o [WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/) no computador. Ele é usado pela janela do aplicativo.

Esses componentes seguem os [pré-requisitos oficiais do Tauri para Windows](https://v2.tauri.app/start/prerequisites/#windows).

Abra um novo PowerShell e confira:

```powershell
git --version
node --version
npm --version
rustc --version
cargo --version
rustup show active-toolchain
```

O resultado da toolchain deve indicar `x86_64-pc-windows-msvc`. Se precisar selecionar a instalação estável MSVC:

```powershell
rustup default stable-x86_64-pc-windows-msvc
```

## 3. Instalar e autenticar as CLIs

Instale somente os fornecedores que pretende usar. O Coucou não cria contas nem copia seus tokens para o banco pessoal. Instalar uma CLI é diferente de autenticar e de qualificar um turno real. As instruções de Gemini e Copilot permitem preparar as CLIs para uso próprio e monitoramento; não habilitam o chat delas no Coucou 0.1.2.

Antes dos comandos de instalação por npm, instale [Node.js 22 ou superior](https://nodejs.org/en/download), mesmo se você recebeu o instalador pronto do Coucou. Abra um novo PowerShell e confira:

```powershell
node --version
npm --version
```

### Codex CLI

Para reproduzir o ambiente de validação, instale **0.159.2**. O contrato experimental de ambiente usado pelo modo pessoal exige uma versão qualificada; o número mostrado pelo terminal deve corresponder ao executável que o Coucou efetivamente inicia:

```powershell
npm install -g @openai/codex@0.159.2
codex --version
codex login
```

Complete o login no navegador. Para usar seu plano ChatGPT, escolha esse fluxo de autenticação. O chat pessoal usa o app-server da CLI instalada. Veja a [documentação oficial do Codex CLI](https://learn.chatgpt.com/docs/codex/cli).

A CLI **0.159.2** foi qualificada com dois turnos reais no modo pessoal, incluindo retomada da mesma sessão após reiniciar o app-server, reutilização de uma concessão de contagem, recusa de uma leitura fora do alvo e ambiente nativo vazio após cada resposta. A aprovação de arquivos foi simulada pelo teste; não comprova clique na janela Tauri. Consulte o relatório de [validação](docs/VALIDACAO-AGENTE-PESSOAL-WINDOWS-2026-10-02.md) para o executável e as evidências. Uma versão diferente pode continuar disponível para o modo projeto, mas o modo pessoal será recusado até qualificação. Não desative essa verificação para contornar um erro.

### Claude Code

Uma forma oficial de instalar no Windows é:

```powershell
winget install Anthropic.ClaudeCode
```

Abra um novo terminal e siga o login interativo:

```powershell
claude --version
claude
```

Use `/login` dentro da CLI se precisar autenticar novamente, e `/exit` para sair. O levantamento local usou 2.1.87; versões instaladas posteriormente exigem nova verificação de compatibilidade. [Instalação e autenticação oficiais](https://code.claude.com/docs/en/quickstart).

### Gemini CLI

```powershell
npm install -g @google/gemini-cli
gemini --version
gemini
```

Complete o fluxo de autenticação apresentado pela CLI, por exemplo com sua conta Google, e encerre a sessão interativa após o login. Contas corporativas podem exigir configuração adicional de projeto Google Cloud. Consulte [instalação](https://geminicli.com/docs/get-started/installation/) e [autenticação](https://geminicli.com/docs/get-started/authentication/).

A versão local detectada foi 0.62.0. O chat Gemini está **indisponível em todos os modos** nesta entrega. A análise da CLI instalada confirmou que hooks globais/de projeto e servidores MCP herdados podem continuar ativos mesmo no modo de planejamento e executar comandos fora das confirmações apresentadas pelo Coucou. Preencher uma pasta de projeto não remove essa limitação. Instalação, login e monitoramento continuam disponíveis. Veja a [referência oficial de hooks do Gemini](https://geminicli.com/docs/hooks/) e o relatório de validação para a análise desta versão.

### GitHub Copilot CLI

Com Node 22+ e PowerShell 7:

```powershell
npm install -g @github/copilot
copilot --version
copilot
```

Digite `/login` na sessão interativa, complete a autenticação GitHub e depois saia. É necessário acesso ao Copilot; organizações podem exigir que a política da CLI esteja habilitada. Esta é a CLI `copilot`, separada da extensão do VS Code e da integração antiga `gh copilot`. [Instalação oficial](https://docs.github.com/en/copilot/how-tos/copilot-cli/set-up-copilot-cli/install-copilot-cli) e [primeiro acesso](https://docs.github.com/en/copilot/get-started/cli-quickstart).

Copilot não estava no PATH no fechamento local; não houve homologação com modelo nem qualificação do isolamento de hooks e MCP. O chat Copilot está **indisponível em todos os modos** até essa verificação. Instalação, login e monitoramento continuam disponíveis. A CLI possui [hooks que executam comandos locais](https://docs.github.com/en/copilot/reference/hooks-reference); limitar ferramentas ou preencher uma pasta não comprova o isolamento necessário.

### Conferir a detecção

```powershell
Get-Command codex, claude, gemini, copilot -ErrorAction SilentlyContinue |
    Select-Object Name, Source
```

Se faltar uma CLI, feche e reabra o terminal e o Coucou para atualizar o ambiente de PATH. Não cole tokens de login no chat, em documentos, no README ou no Git.

Se há mais de uma instalação do Codex, confira todos os caminhos:

```powershell
Get-Command codex, codex.exe -All -ErrorAction SilentlyContinue |
    Select-Object Name, CommandType, Source
where.exe codex
npm prefix -g
```

O lançador npm (`codex.cmd`/`codex.ps1`) e um `codex.exe` incluído no aplicativo Codex Desktop podem apontar para versões distintas. Registre os caminhos e compare com a CLI detectada no Coucou. Não remova executáveis nem altere configurações globais para tentar contornar uma divergência; a versão e o protocolo devem ser verificados no binário selecionado.

## 4. Compilar e gerar o instalador

Prepare os pré-requisitos de compilação da seção 2. Abra o PowerShell na pasta onde quer guardar o projeto e clone o branch completo. O exemplo cria uma subpasta `coucou` no local atual e depois entra em `windows`; funciona sem depender de um caminho ou unidade específicos:

```powershell
git clone --branch codex/multiagent-windows --single-branch https://github.com/OsmarZM/coucou.git coucou
Set-Location -LiteralPath '.\coucou\windows'
npm ci
npm run pack
```

Se já tem um checkout, confira o branch e preserve as alterações com `git branch --show-current` e `git status --short` antes de entrar na pasta `windows`. Para uma cópia em ZIP, baixe o branch indicado e extraia o repositório inteiro: o build usa também os 28 sons versionados em `NotchBuddy/Resources/sounds`, fora de `windows`.

Para executar as verificações locais, use o PowerShell na mesma pasta `windows`:

```powershell
npm test
cargo test --workspace
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
```

`npm ci` usa o lockfile. O comando de empacotamento compila o hook, verifica TypeScript, gera o frontend, compila o backend e produz o instalador NSIS. O primeiro build baixa dependências e pode levar vários minutos.

Arquivos produzidos:

```text
.\release\Coucou-Windows-0.1.2-setup.exe
.\release\Coucou-Windows-setup.exe
```

O primeiro nome identifica a versão; o segundo aponta para o último pacote gerado localmente. `release/`, `target/` e `node_modules/` não são versionados. Gere o pacote quando usar o código em outro computador. Não use os executáveis antigos da raiz como substitutos do pacote novo.

Para desenvolvimento nativo, execute também na pasta `windows` do checkout:

```powershell
npm run tauri dev
```

`npm run dev` sozinho abre apenas o servidor frontend; não inicia o backend Tauri.

## 5. Instalar e abrir o Coucou

1. Termine as execuções ativas do Coucou e saia pelo ícone da bandeja antes de atualizar.
2. Abra `Coucou-Windows-0.1.2-setup.exe` por duplo clique. No checkout, ele fica em `windows\release`; após os comandos da seção 4, o caminho no terminal é `.\release\Coucou-Windows-0.1.2-setup.exe`.
3. Siga o assistente de instalação para o usuário atual. O pacote inclui português brasileiro.
4. Abra o Coucou pelo menu Iniciar/atalho criado pelo instalador.
5. Abra **Configurações** e confira quais CLIs foram detectadas.
6. Abra o chat pelo ícone de conversa. Escolha um personagem compatível e envie uma mensagem curta para confirmar o login no aplicativo. Esse envio usa sua conta do fornecedor e pode consumir a cota do plano.

A instalação do Coucou não instala as CLIs, não autentica suas contas e não ativa todos os hooks automaticamente. O fluxo de hooks apresenta prévia das alterações por fornecedor; revise antes de instalar e preserve handlers de terceiros. Veja o [guia multiagente](docs/MULTIAGENTE-WINDOWS.md).

## 6. Usar a conversa pessoal

1. Abra o chat e escolha o personagem Codex ou Claude. Escolher o personagem não envia uma mensagem.
2. Converse normalmente. Histórico, rascunho, preferências e anexos pessoais são compartilhados. Não existe chave para ativar aprendizado.
3. No Codex, experimente “Quantos arquivos tenho em Downloads?”. A operação mostra alvo e permissão; não é necessário preencher projeto.
4. Arraste PDF textual, DOCX, texto/código, CSV ou JSON para a ilha, ou use **Anexar arquivos**. Confira estado e cobertura antes do envio.
5. Use **Contexto** para consultar ou corrigir memórias e histórico e revisar procedimentos. Conhecimento lembrado não libera ferramentas.
6. Use **Avançado** para projeto ou retomada manual dos fornecedores habilitados. Um chat externo exige autorização para cada mensagem, com destino e conteúdo integral. Gemini e Copilot exibem a limitação de chat também nesse modo.

A ilha compacta permanece visível por padrão. A ocultação/fixação é uma escolha do usuário; quando oculta, a área no topo central permite revelá-la pelo cursor. Os cards apresentam métricas reportadas pelas CLIs; dados indisponíveis não aparecem como saldo zero. Consulte o [guia de uso](docs/AGENTE-PESSOAL-WINDOWS.md) e a [política de comunicação entre chats](docs/CONTROLE-DE-MENSAGENS-ENTRE-CHATS.md).

| Fornecedor | Conversa pessoal | Operações sobre arquivos no modo pessoal | Evidência local |
|---|---|---|---|
| Codex CLI | Implementada, limitada ao protocolo qualificado | Contar, listar e ler por ferramentas mediadas pelo Coucou, com permissão | 0.159.2: dois turnos reais, retomada e escopo de arquivos; aprovações simuladas |
| Claude Code | Implementada com ferramentas nativas desativadas | Pode receber documentos preparados; não usa as ferramentas pessoais de filesystem do Codex | Detecção de 2.1.87; sem turno real homologado nesta entrega |
| Gemini CLI | Chat indisponível em todos os modos; monitoramento disponível | Isolamento de hooks/MCP ainda não qualificado | Detecção de 0.62.0; análise confirmou configurações herdadas ativas |
| GitHub Copilot CLI | Chat indisponível em todos os modos; monitoramento disponível | Isolamento de hooks/MCP ainda não qualificado | CLI ausente no PATH local; sem turno real homologado |

O histórico pessoal e as memórias informam os turnos dentro de um orçamento de contexto; mensagens antigas podem ser recortadas ou omitidas. Isso não treina os pesos do modelo. A troca de personagem preserva o contexto compartilhado, mas as permissões continuam vinculadas ao fornecedor e canal. Não há troca automática de fornecedor após um erro.

A opção existente **Anthropic API** continua separada: ela exige uma chave configurada em **Configurações**, usa o Windows Credential Manager e pode gerar cobrança na conta de API. O login do Claude Code não substitui essa chave.

## 7. Dados, atualização e problemas comuns

O histórico e as memórias ficam em `%LOCALAPPDATA%\Coucou\personal\state.db`; as cópias dos documentos ficam em `%LOCALAPPDATA%\Coucou\documents`. Credenciais continuam sob responsabilidade das CLIs. A migração atualiza a política de contexto contínuo e preserva os registros existentes.

O clone do GitHub leva o código, os testes e os recursos versionados. Histórico, memórias, anexos pessoais, configurações do perfil Windows, credenciais e sessões nativas das CLIs são dados locais e não vêm no clone. No outro computador, faça o login de cada CLI novamente e revise a instalação dos hooks pelo Coucou. Transferir dados pessoais existentes exige um backup separado; a migração entre computadores ainda não foi homologada.

| Problema | Verificação |
|---|---|
| `cargo`/`node` não encontrado | Reabra o terminal e confira instalação/PATH |
| `link.exe` ou Windows SDK ausente | Instale o workload C++ das Build Tools e use Rust MSVC |
| CLI não aparece no Coucou | Confira `Get-Command`, login no terminal e reinicie o aplicativo |
| Codex pessoal rejeita versão | Confira os caminhos de `Get-Command -All` e a versão efetiva iniciada; siga a versão/qualificação indicada no relatório |
| Gemini/Copilot não envia mensagens | Chat indisponível nesta versão até qualificar o isolamento de hooks/MCP; selecionar projeto não habilita o envio |
| Cota/processos/tokens indisponíveis | O fornecedor precisa expor esses dados; instalar não cria uma fonte de métricas |
| Anexo parcial ou não suportado | Confira cobertura; imagens/OCR, PDF só digitalizado e DOC legado não têm leitura nesta versão |

Esquecer um registro no Coucou não apaga sessões/logs mantidos pelos fornecedores nem os arquivos originais. Para desinstalar, use **Aplicativos instalados → Coucou**; revise/remova os hooks pelo fluxo próprio antes de retirar o aplicativo. Não apague configurações globais de fornecedores para tentar limpar o Coucou.

## 8. Validar alterações futuras

Os testes com CLI real são separados dos testes automáticos. Execute os comandos desta seção na pasta `windows` do checkout. O smoke pessoal usa duas mensagens reais no plano ChatGPT, em uma sessão criada pelo teste, com aprovação de filesystem simulada:

```powershell
$env:COUCOU_RUN_PERSONAL_SMOKE = '1'
cargo test -p coucou --lib personal_tools_are_scoped_and_resume_without_a_native_environment -- --ignored --nocapture
```

Execute conscientemente após autenticar e verificar a versão. Esse teste não comprova clique de permissão na janela nativa. Para verificar o worker de documentos após um build:

```powershell
node scripts/probe-documents-worker.mjs --executable=./target/release/coucou.exe
```

O relatório de [validação de 02/10/2026](docs/VALIDACAO-AGENTE-PESSOAL-WINDOWS-2026-10-02.md) separa testes, prévia no navegador, CLI real, pacote e as verificações nativas ainda pendentes.
