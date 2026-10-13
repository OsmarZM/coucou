# Coucou pessoal no Windows — instalação passo a passo

## Objetivo e escopo

Instalar o Coucou 0.1.4, preparar os personagens e, se desejar, configurar o runtime real do Hermes como supervisor. Codex e Claude mantêm seus canais diretos. O personagem Hermes pode responder com uma conexão de modelo configurada e solicitar autorização para criar auxiliares Codex ou Claude. Gemini CLI e GitHub Copilot CLI podem ser instalados e monitorados; seu chat permanece indisponível até a qualificação do isolamento de hooks e MCP.

A guia principal concentra a escolha dos personagens. A conversa pessoal compartilha histórico, preferências e anexos entre os canais compatíveis. Não é necessário ativar aprendizado ou escolher uma pasta para começar. O modo gratuito e o supervisor vêm desabilitados: configurar ou abrir uma tela não inicia uma pergunta com modelo.

O repositório desta implementação é [OsmarZM/coucou — branch codex/multiagent-windows](https://github.com/OsmarZM/coucou/tree/codex/multiagent-windows). Escolha livremente a pasta do projeto em seu computador; não é necessário ter uma unidade `D:`. O instalador deve ser gerado pelos comandos da seção 4. O workflow Windows mantém `PUBLISH=false`, com a publicação automática de instalador pausada. Consulte a [validação Hermes/FreeLLMAPI de 06–08/10/2026](docs/VALIDACAO-HERMES-FREELLMAPI-WINDOWS-2026-10-06.md) para distinguir implementação, testes locais e verificações ainda pendentes. A [validação pessoal anterior](docs/VALIDACAO-AGENTE-PESSOAL-WINDOWS-2026-10-02.md) conserva a evidência das CLIs e das versões anteriores.

## 1. Escolher a forma de instalar

| Situação | Caminho |
|---|---|
| Você recebeu o instalador 0.1.4 | Instale as CLIs desejadas, seguindo a seção 3, e o aplicativo, na seção 5 |
| Você tem o código e quer gerar o instalador | Prepare o ambiente na seção 2 e execute a seção 4 |
| Você quer desenvolver e testar a janela Tauri | Siga as seções 2 a 4 e use `npm run tauri dev` |

O aplicativo instalado não precisa de Rust, Visual Studio, Docker ou banco de dados externo. As ferramentas de compilação são necessárias apenas para gerar o aplicativo a partir do código. Os fornecedores continuam exigindo sua própria autenticação e plano/acesso correspondente. Hermes e FreeLLMAPI têm runtimes privados, instalados explicitamente pela configuração opcional; não são instalados só por abrir o Coucou.

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

Instale somente os fornecedores que pretende usar. O Coucou não cria contas nem copia seus tokens para o banco pessoal. Instalar uma CLI é diferente de autenticar e de qualificar um turno real. As instruções de Gemini e Copilot permitem preparar as CLIs para uso próprio e monitoramento; não habilitam o chat delas no Coucou 0.1.4. Hermes utiliza uma conexão de modelo própria: o login do Codex ou do Claude não autentica automaticamente o supervisor.

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

Se já tem um checkout, confira o branch e preserve as alterações com `git branch --show-current` e `git status --short` antes de entrar na pasta `windows`. Para uma cópia em ZIP, baixe o branch indicado e extraia o repositório inteiro: o build usa também os sons versionados em `NotchBuddy/Resources/sounds`, fora de `windows`.

Para executar as verificações locais, use o PowerShell na mesma pasta `windows`:

```powershell
npm test
cargo test --workspace
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
```

`npm ci` usa o lockfile. O comando de empacotamento compila o hook, verifica TypeScript, gera o frontend, compila o backend e produz os instaladores NSIS e MSI. O primeiro build baixa dependências e pode levar vários minutos.

Arquivos produzidos:

```text
.\release\Coucou-Windows-0.1.4-setup.exe
.\release\Coucou-Windows-setup.exe
.\release\Coucou-Windows-0.1.4.msi
.\release\Coucou-Windows.msi
```

Os nomes com `0.1.4` identificam a versão; os nomes sem versão apontam para o último pacote gerado localmente. Escolha um dos formatos de instalador. `release/`, `target/` e `node_modules/` não são versionados. Gere o pacote quando usar o código em outro computador. Não use os executáveis antigos da raiz como substitutos do pacote novo.

Para desenvolvimento nativo, execute também na pasta `windows` do checkout:

```powershell
npm run tauri dev
```

`npm run dev` sozinho abre apenas o servidor frontend; não inicia o backend Tauri.

## 5. Instalar e abrir o Coucou

Se a janela ainda mostra uma lista “Codex CLI”, “Choose folder” ou textos em inglês, confira qual executável abriu. Os executáveis antigos na raiz são preservados e não são atualizados por `git pull` ou pela compilação. Após gerar o pacote, instale a versão 0.1.4 ou execute `Iniciar-Coucou.ps1` na raiz do checkout. O lançador abre `windows\target\release\coucou.exe` e pede que você saia de outra cópia aberta; não encerra processos automaticamente. O rodapé do chat mostra a versão efetiva do aplicativo.

1. Termine as execuções ativas do Coucou e saia pelo ícone da bandeja antes de atualizar.
2. Abra `Coucou-Windows-0.1.4-setup.exe` por duplo clique. No checkout, ele fica em `windows\release`; após os comandos da seção 4, o caminho no terminal é `.\release\Coucou-Windows-0.1.4-setup.exe`.
3. Siga o assistente de instalação para o usuário atual. O pacote inclui português brasileiro.
4. Abra o Coucou pelo menu Iniciar/atalho criado pelo instalador.
5. Abra **Configurações** e confira quais CLIs foram detectadas.
6. Na guia principal, clique no personagem Codex ou Claude para abrir a conversa. Envie uma mensagem curta para confirmar o login no aplicativo. Esse envio usa sua conta do fornecedor e pode consumir a cota do plano. Para conversar com Hermes, configure primeiro a seção 7.

A instalação do Coucou não instala as CLIs, não autentica suas contas e não ativa todos os hooks automaticamente. O fluxo de hooks apresenta prévia das alterações por fornecedor; revise antes de instalar e preserve handlers de terceiros. Veja o [guia multiagente](docs/MULTIAGENTE-WINDOWS.md).

## 6. Usar a conversa pessoal

1. Na guia principal, escolha Codex, Claude ou o personagem pessoal Hermes configurado. O chat mostra o nome/avatar ativo, sem outra lista de CLI. Escolher o personagem não envia uma mensagem.
2. Converse normalmente. Histórico, rascunho, preferências e anexos pessoais são compartilhados. Não existe chave para ativar aprendizado.
3. No Codex, experimente “Quantos arquivos tenho em Downloads?”. A operação mostra alvo e permissão; não é necessário preencher projeto.
4. Arraste PDF textual, DOCX, texto/código, CSV ou JSON para a ilha, ou use **Escolher arquivos**/**Anexar arquivos**. A cópia mostra **Importando…** até receber o resultado real; ela oferece cancelamento e tem prazo de 60 segundos. Confira estado e cobertura antes do envio. Cada contexto aceita até 10 anexos; cada arquivo, até 20 MiB; cada lote, até 50 MiB.
5. Após anexar, escolha **Ler e resumir** ou **Fazer uma pergunta**. A primeira ação da guia de arquivos abre a conversa com um pedido de resumo; o envio acontece ao clicar em **Enviar** ou **Ler e resumir** no chat. Arrastar não envia conteúdo automaticamente. Imagens podem ser anexadas, mas visão/OCR e PDF somente digitalizado ainda não têm leitura disponível.
6. Use **Contexto** para consultar ou corrigir memórias e histórico e revisar procedimentos. Conhecimento lembrado não libera ferramentas.
7. Use **Avançado** para projeto ou retomada manual dos fornecedores habilitados. Um chat externo exige autorização para cada mensagem, com destino e conteúdo integral. Gemini e Copilot exibem a limitação de chat também nesse modo. Hermes fica restrito ao contexto pessoal e não retoma chats externos por delegação.

A ilha compacta permanece visível por padrão. A ocultação/fixação é uma escolha do usuário; quando oculta, a área no topo central permite revelá-la pelo cursor. Os cards apresentam métricas reportadas pelas CLIs; dados indisponíveis não aparecem como saldo zero. Consulte o [guia de uso](docs/AGENTE-PESSOAL-WINDOWS.md) e a [política de comunicação entre chats](docs/CONTROLE-DE-MENSAGENS-ENTRE-CHATS.md).

| Fornecedor | Conversa pessoal | Operações sobre arquivos no modo pessoal | Evidência local |
|---|---|---|---|
| Codex CLI | Implementada, limitada ao protocolo qualificado | Contar, listar e ler por ferramentas mediadas pelo Coucou, com permissão | 0.159.2: dois turnos reais, retomada e escopo de arquivos; aprovações simuladas |
| Claude Code | Implementada com ferramentas nativas desativadas | Pode receber documentos preparados; não usa as ferramentas pessoais de filesystem do Codex | Detecção de 2.1.87; sem turno real homologado nesta entrega |
| Gemini CLI | Chat indisponível em todos os modos; monitoramento disponível | Isolamento de hooks/MCP ainda não qualificado | Detecção de 0.62.0; análise confirmou configurações herdadas ativas |
| GitHub Copilot CLI | Chat indisponível em todos os modos; monitoramento disponível | Isolamento de hooks/MCP ainda não qualificado | CLI ausente no PATH local; sem turno real homologado |
| Hermes | Adapter ACP do runtime real, exige instalação e conexão próprias | Ponte tipada para contexto, arquivos, documentos, propostas e auxiliares Codex/Claude; permissão continua no Rust | Runtime real e transporte qualificados com modelo local em 08/10; configuração não comprova inferência com um provedor remoto |

O histórico pessoal e as memórias informam os turnos dentro de um orçamento de contexto; mensagens antigas podem ser recortadas ou omitidas. Isso não treina os pesos do modelo. A troca de personagem preserva o contexto compartilhado, mas as permissões continuam vinculadas ao fornecedor e canal. Não há troca automática de fornecedor após um erro.

### Quem coordena cada personagem?

Escolher **Hermes** inicia seu runtime ACP sob demanda quando você envia uma mensagem, usando a conexão configurada. Ele pode solicitar até dois auxiliares por turno, com profundidade de uma camada; cada delegação cria uma sessão nova, somente leitura, e exige aprovação para o objetivo integral. O cancelamento do turno alcança os filhos gerenciados.

Escolher **Codex** ou **Claude** mantém o canal direto existente. O contexto pessoal continua compartilhado, mas esses botões não passam automaticamente por Hermes nesta entrega. Resend, n8n, Vercel e GitHub são integrações de serviços, não outros modelos de linguagem. Gemini e Copilot permanecem bloqueados para chat. Essa distinção evita apresentar como implementada uma coordenação universal ainda inexistente.

O runtime integrado do Hermes formula respostas e propostas de memória/procedimentos através da ponte. O Coucou/Rust mantém a escrita persistente, a política de origem e a revisão em **Contexto**; espelha o contexto aprovado no perfil privado Hermes. As ferramentas nativas de memória/skills, terminal, código, plugins e MCP herdados do Hermes ficam desativadas. O aprendizado nativo completo do projeto Hermes não está habilitado nesse perfil; esta adaptação conserva os controles de permissão e uma única fonte persistente de memória.

A configuração existente **Anthropic API** é separada: exige chave própria no Windows Credential Manager e pode gerar cobrança na conta de API. O login do Claude Code não substitui essa chave. Seu botão não fica na seleção cotidiana de personagens do chat.

## 7. Configurar Hermes e o modo gratuito opcional

Hermes é o runtime supervisor. FreeLLMAPI é um roteador local para as contas de provedores escolhidas por você. Cada CLI auxiliar continua usando seu próprio login e sua própria cota. O anúncio agregado de bilhões de tokens do FreeLLMAPI não representa crédito garantido para sua conta. A inferência é remota mesmo com um servidor local.

### 7.1 Instalar o runtime privado Hermes

1. Abra **Configurações → Hermes — supervisor dos agentes**.
2. Use **Instalar runtime Hermes fixado**. O download é explícito e não consulta um modelo. O runtime inclui Python e dependências ACP/MCP próprios, sem instalar hooks ou alterar seu Hermes global.
3. Confira os campos **Python privado** e **Origem Hermes** retornados pela instalação. A revisão exigida é `781334eea4b9225a3e194faf0c241d9afe218634`.
4. Escolha a conexão: serviço gratuito configurado na seção 7.2 ou endpoint alternativo na seção 7.3.
5. Em **Contexto do modelo em tokens**, informe a capacidade documentada do modelo escolhido. A revisão Hermes exige pelo menos **64.000 tokens**, com limite de configuração de 1.000.000 nesta integração. O campo começa em 64.000; confirme que o modelo suporta esse valor. Ele não mede uso nem aumenta a capacidade do provedor. Marque **Ativar o supervisor Hermes** e clique em **Salvar supervisor**. A tela confirma configuração; as ferramentas efetivas são conferidas antes de cada turno. Uma instalação incompatível ou ferramenta extra impede o envio do prompt.

Se precisar executar a instalação pelo código, na pasta `windows` do checkout:

```powershell
$hermesRuntime = Join-Path $env:LOCALAPPDATA 'Coucou\managed-services\hermes\runtime'
& .\scripts\managed-services\provision-hermes.ps1 -Destination $hermesRuntime
```

O script imprime `pythonExecutable` e `sourceDirectory`. Preencha esses valores absolutos na tela Hermes. O caminho habitual do Python é `runtime\source\.venv\Scripts\python.exe`; a origem é `runtime\source`. Não use o launcher de um Hermes global para preencher esses campos. O script preserva um destino já existente e recusa sobrescrevê-lo; falhas deixam logs e a instalação parcial para diagnóstico.

A primeira instalação baixa e verifica fonte, Python e dependências. O setup dos runtimes tem prazo máximo de 60 minutos; esse limite não é uma previsão de duração. Nesta máquina, sob forte concorrência de disco, o setup frio do FreeLLMAPI levou aproximadamente 48 minutos. Uma verificação independente dos 14.231 arquivos levou aproximadamente 11 minutos; isso não é um benchmark do verificador Rust do aplicativo. A verificação completa de integridade precede cada partida do serviço e pode ser demorada em disco lento. O bootstrap Node teve inicializações medidas entre 14 e 30 segundos depois de instalado, sem incluir a verificação de integridade; a tentativa mais fria demorou mais, e a espera de prontidão do processo aceita até 120 segundos. Esses tempos não medem latência de um modelo remoto. Consultar o estado do serviço continua disponível enquanto os downloads ocorrem.

Se uma instalação falhar e a próxima tentativa informar que o destino existe, feche o Coucou, confira os logs na pasta desse runtime e preserve a pasta parcial com outro nome antes de tentar novamente. Para isso, renomeie somente `managed-services\hermes\runtime` ou `managed-services\free-models\runtime-a6b2158c7c36ce19f888d0411846a1a0f2aa3f06`, conforme o setup que falhou. Confira o caminho realmente criado antes de renomear. Não renomeie a pasta `personal`, `documents`, `profiles` ou `data`, que pode conter informações persistentes. Um destino incompatível é recusado; não desative a verificação de integridade para reaproveitá-lo.

### 7.2 Usar modelos com cotas gratuitas

1. Em **Configurações → Agente pessoal e modelos com cotas gratuitas**, escolha **Configurar agora**. **Continuar sem ativar** salva sua escolha e mantém os canais existentes. Você pode configurar depois.
2. Informe o **Nome do agente**. Esse nome altera o personagem pessoal; a origem do processamento permanece identificada como Hermes.
3. Escolha **Groq** ou **Cerebras** e informe o identificador exato de um modelo disponível em sua conta, com contexto documentado de pelo menos 64.000 tokens para o supervisor Hermes. Seleção `auto`, fallback e troca para outro provedor estão desabilitados.
4. Clique em **Salvar configuração** para guardar o nome, o provedor e o modelo antes de cadastrar a chave. Crie sua chave no fluxo oficial do provedor: [Groq — primeiros passos](https://console.groq.com/docs/quickstart) ou [Cerebras — primeiros passos](https://inference-docs.cerebras.ai/quickstart). Confira os limites e o acesso da sua conta. Guarde a chave no campo **Credencial própria → Guardar chave** do Coucou; não no chat, `.env` versionado ou README.
5. Clique em **Instalar runtime fixado**. O download instala FreeLLMAPI na revisão `a6b2158c7c36ce19f888d0411846a1a0f2aa3f06` com Node `22.22.0` privado. Ele não exige um Node global adicional para executar o serviço instalado.
6. Clique em **Habilitar modo gratuito** e **Iniciar serviço**. Confira **Serviço: em execução**. Isso confirma a infraestrutura local; não comprova que o modelo remoto suporta todas as ferramentas.
7. Se quiser, marque **Iniciar apenas este serviço ao entrar no Windows** e salve. Salvar uma configuração com o serviço em execução o para; use **Iniciar serviço** novamente quando quiser continuar na sessão atual.
8. Na seção Hermes, marque **Usar o serviço de modelos gratuitos configurado acima**, confira o campo **Contexto do modelo em tokens**, ative o supervisor e salve. Na guia principal, escolha seu personagem pessoal e envie uma pergunta curta, sem documentos sensíveis, para verificar a conexão real.

A credencial do provedor, a chave de transporte local e a chave mestra ficam protegidas pelo Windows Credential Manager; não são devolvidas ao frontend. O servidor escuta em `127.0.0.1`, exige autenticação e passa por confirmação de identidade antes de receber credenciais. Ele não inicia o desktop do FreeLLMAPI nem os jobs de inferência padrão do projeto.

**Parar serviço** encerra apenas a infraestrutura própria. **Desabilitar modo gratuito** remove a preferência de autostart desse serviço, preservando a configuração independente de iniciar o Coucou. A entrada específica é `CoucouFreeModels`, no perfil do usuário Windows. Boot/login inicia somente o servidor habilitado: não inicia Hermes, não envia perguntas, não retoma tarefas nem orienta chats. Para alterar uma chave, pare o serviço primeiro.

### 7.3 Usar um endpoint alternativo

Você pode instalar Hermes sem habilitar FreeLLMAPI. Desmarque **Usar o serviço de modelos gratuitos configurado acima** e informe um endpoint compatível com chat completions, terminado em `/v1`, o modelo exato e uma chave própria em **Guardar chave**. Confira a capacidade documentada do modelo no campo **Contexto do modelo em tokens**; Hermes exige no mínimo 64.000. São aceitos HTTPS ou HTTP de loopback local; URLs com credenciais ou parâmetros de segredo são recusadas. Salve e ative o supervisor.

O endpoint escolhido pode gerar cobrança segundo a conta que você configurou. Login OAuth/assinatura de um Hermes global e credenciais das CLIs não são importados. A documentação oficial do [ACP Hermes](https://hermes-agent.nousresearch.com/docs/user-guide/features/acp/) explica o transporte; o perfil Coucou aplica sua própria política restrita.

### 7.4 Verificar uma delegação

Com Hermes conectado e uma CLI compatível autenticada, peça uma análise simples a um auxiliar Codex ou Claude. Confira o card **Autorizar agente auxiliar?**, o executor e o objetivo integral antes de permitir. Negar não deve criar o auxiliar. Os auxiliares desta entrega são somente leitura, usam sessões novas e executam até dois pedidos por turno de forma sequencial. O supervisor recebe o resultado para compor sua resposta.

Não há aprovação permanente para mensagens externas. A ponte Hermes não oferece ferramenta para orientar chats já abertos. O modo avançado direto conserva sua autorização única por mensagem e o bloqueio de duplicatas. Se ocorrer timeout ou resultado incerto, confira o estado registrado antes de decidir por um novo envio; o Coucou não repete automaticamente a tarefa.

## 8. Dados, atualização e problemas comuns

O histórico e as memórias ficam em `%LOCALAPPDATA%\Coucou\personal\state.db`; as cópias dos documentos ficam em `%LOCALAPPDATA%\Coucou\documents`. Hermes usa `%LOCALAPPDATA%\Coucou\managed-services\hermes` para configuração/runtime e `profiles\personal-main` para o perfil exclusivo. FreeLLMAPI usa `%LOCALAPPDATA%\Coucou\managed-services\free-models` para configuração, runtime e dados. Credenciais das CLIs continuam sob responsabilidade de cada fornecedor. Configurações gerais continuam no perfil do Coucou; não altere essas pastas manualmente enquanto houver execução ativa.

O clone do GitHub leva o código, os testes, os scripts de provisionamento e os recursos versionados. Histórico, memórias, anexos pessoais, configurações do perfil Windows, credenciais, runtimes baixados e sessões nativas das CLIs são dados locais e não vêm no clone. No outro computador, faça o login de cada CLI novamente, instale os runtimes opcionais e guarde suas chaves no cofre daquela máquina. Revise a instalação dos hooks pelo Coucou. Transferir dados pessoais existentes exige um backup separado; não copie segredos do Credential Manager como se fossem arquivos portáteis. A migração entre computadores ainda não foi homologada.

| Problema | Verificação |
|---|---|
| `cargo`/`node` não encontrado | Reabra o terminal e confira instalação/PATH |
| `link.exe` ou Windows SDK ausente | Instale o workload C++ das Build Tools e use Rust MSVC |
| CLI não aparece no Coucou | Confira `Get-Command`, login no terminal e reinicie o aplicativo |
| Codex pessoal rejeita versão | Confira os caminhos de `Get-Command -All` e a versão efetiva iniciada; siga a versão/qualificação indicada no relatório |
| Gemini/Copilot não envia mensagens | Chat indisponível nesta versão até qualificar o isolamento de hooks/MCP; selecionar projeto não habilita o envio |
| Cota/processos/tokens indisponíveis | O fornecedor precisa expor esses dados; instalar não cria uma fonte de métricas |
| Anexo parcial ou não suportado | Confira cobertura; imagens/OCR, PDF só digitalizado e DOC legado não têm leitura nesta versão |
| Importação falhou/cancelou/excedeu prazo | Erro permanece visível; confira anexos já presentes antes de tentar de novo. Caminhos de rede, links/reparse e arquivos indisponíveis exigem diagnóstico próprio |
| Hermes indisponível | Confira revisão fixada, Python `source\.venv\Scripts\python.exe`, serviço/conexão, chave e opção de ativar supervisor; configurar não comprova turno com modelo |
| Serviço gratuito não inicia | Confira runtime, modelo exato, provedor autorizado e chave. Arquivos ou manifesto alterados são recusados; não desative a verificação de integridade |
| Runtime já existe após falha de setup | Confira os logs do destino preservado. Não apague nem sobrescreva outra instalação; use um novo destino explícito e configure os caminhos retornados |
| Cota gratuita esgotada/429 | Aguarde o reset informado pelo provedor ou escolha conscientemente outra configuração. Não há fallback automático nem saldo pessoal de bilhões de tokens |
| Serviço não volta no login | Confira se está habilitado e a opção de inicialização foi salva para o executável instalado atual; instalação/upgrade real ainda requer homologação |

Esquecer um registro no Coucou não apaga sessões/logs mantidos pelos fornecedores nem os arquivos originais. Para desinstalar, use **Aplicativos instalados → Coucou**; revise/remova os hooks pelo fluxo próprio antes de retirar o aplicativo. Não apague configurações globais de fornecedores para tentar limpar o Coucou.

## 9. Validar alterações futuras

Os testes com CLI real são separados dos testes automáticos. Execute os comandos desta seção na pasta `windows` do checkout. O smoke pessoal usa duas mensagens reais no plano ChatGPT, em uma sessão criada pelo teste, com aprovação de filesystem simulada:

```powershell
$env:COUCOU_RUN_PERSONAL_SMOKE = '1'
cargo test -p coucou --lib personal_tools_are_scoped_and_resume_without_a_native_environment -- --ignored --nocapture
```

Execute conscientemente após autenticar e verificar a versão. Esse teste não comprova clique de permissão na janela nativa. Para verificar o worker de documentos após um build:

```powershell
node scripts/probe-documents-worker.mjs --executable=./target/release/coucou.exe
```

O relatório de [06–08/10/2026](docs/VALIDACAO-HERMES-FREELLMAPI-WINDOWS-2026-10-06.md) separa os gates Hermes/FreeLLMAPI, anexos nativos, modelos remotos, autostart e pacote. O relatório de [02/10/2026](docs/VALIDACAO-AGENTE-PESSOAL-WINDOWS-2026-10-02.md) conserva a evidência pessoal anterior; ela não homologa automaticamente uma conexão nova do supervisor.
