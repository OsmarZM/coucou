# Validação do Coucou Agent Runtime — 09/10/2026

## Objetivo e contexto

Validar a primeira fase descrita em [COUCOU-AGENT-RUNTIME.md](COUCOU-AGENT-RUNTIME.md). O Git já tinha alterações extensas e arquivos não rastreados antes desta tarefa; foram preservados. Não houve commit, push, instalação, publicação ou alteração de hooks/configurações globais. Executáveis existentes na raiz não foram substituídos.

## Evidências locais

| Verificação | Resultado |
| --- | --- |
| `npm test` | 123 aprovados, zero falhas, um opt-in ignorado. Inclui disponibilidade do runtime e regressões de seleção, rascunhos, foco e cartões de autorização. |
| `npx tsc --noEmit` | Aprovado. |
| `npx vite build` | Aprovado; 48 módulos transformados. |
| `cargo test -p coucou --lib agent_runtime` | 12 aprovados, zero falhas na versão final do módulo. |
| `cargo test --workspace` | Rodada após proteção dos resultados: 171 aprovados, zero falhas, três opt-in ignorados. |
| `cargo test -p coucou --lib runtime` | 19 aprovados, zero falhas na conferência focal final do runtime e do adapter, incluindo propagação de saída CLI não zero como falha de ferramenta. O filtro também inclui cinco testes anteriores de módulos com runtime no nome. |
| `cargo clippy --workspace --all-targets -- -D warnings` | Aprovado, inclusive após proteção dos resultados e IDs únicos de sessão. |
| `cargo fmt --all --check` | Conferido na rodada final. |
| `git diff --check` | Aprovado; avisos de conversão LF/CRLF são do checkout. |
| `node scripts/probe-personal-protocol.mjs` | Codex instalado 0.161.0 respondeu com `environments: []`, sandbox `readOnly` e `modelTurnStarted: false`; sem leitura de conta ou alterações globais. É evidência parcial do protocolo, não qualificação completa de isolamento. |

## Cobertura do módulo

- Registro recusa ferramenta inexistente, shell, comando livre, campos adicionais, enums inválidos e caminhos com caracteres de controle.
- Todas as ferramentas declaradas aceitam seu formato tipado esperado; o schema proíbe campos adicionais.
- Runtime solicita autorização única; negação não consulta processos. Chamadas inválidas e auxiliares sem habilitação não chegam à autorização.
- Consulta autorizada de processos usa um snapshot real do Windows e encontra o próprio processo de teste. O clique humano é substituído por fixture.
- Cancelamento impede despacho e interrompe sessão em andamento. Timeout interrompe processo bloqueado.
- Captura de processo real de fixture coleta stdout, stderr e código de saída 7 sem transformá-lo em sucesso.
- Saída acima de 64 KiB por stream é drenada e sinalizada; resultado incompleto não é aceito.
- Detector existente de credenciais e orçamento de 256 KiB bloqueiam resultados antes do modelo.
- Plano reconhece destino ocupado e colisão de nomes sem diferenciar maiúsculas/minúsculas. Teste com arquivos reais confirma bytes preservados e nenhuma movimentação.
- Caminhos UNC, dispositivos, ADS, traversal e tentativa de comando em nome de pasta são rejeitados. A suíte existente também cobre junction/reparse, hardlinks, substituição de handles e revogação.
- Adapter consome IDs de chamadas novas, recusa replay e limita novas ações a 16 por turno.
- Processo CLI com saída não zero conserva seu resultado estruturado e gera ferramenta/atividade com sucesso falso, sem transformar coleta bem-sucedida em execução bem-sucedida.
- Testes existentes dos hooks/protocolo permanecem aprovados.

Os testes iniciais expuseram duas falhas nas fixtures: identidade de destino com separadores diferentes e uso direto de um caminho temporário Windows com nome curto. A correção normaliza a identidade do plano e usa o resolver de caminhos do broker antes de abrir handles. As fixtures foram repetidas e passaram.

## Fornecedores e limites

Codex, Claude e Gemini foram detectados no PATH. `codex --version` retornou **0.161.0**. O chat pessoal continua com a guarda prévia **0.159.2**; não foi alterada para aceitar uma versão sem qualificação. A geração local dos schemas do app-server foi uma inspeção sem prompt, chamada de modelo ou leitura de login. A ferramenta de versão não habilita o chat Gemini nem altera sua política de isolamento.

Não foi executado turno real de modelo, delegação real, leitura da lixeira pessoal ou abertura real de aplicativos/configurações. A autorização do runtime foi exercitada por fixture, não por clique no cartão Tauri. `launch_requested` representa apenas solicitação de processo; a janela visível não foi verificada.

O Linux completo permanece **pendente**: não há target Rust Linux instalado e a consulta ao Ubuntu WSL não encontrou Cargo/Rust. O módulo novo isola Win32 sob `cfg(windows)` e retorna catálogo vazio fora do Windows. Imports Windows anteriores e incondicionais no checkout ainda precisam de tratamento/qualificação para a build Linux; não existe PASS Linux nesta evidência.

## Reproduzir

Executar os comandos da tabela dentro de `D:\coucou\windows`. Os scripts PowerShell usados nos testes de sessão são fixtures constantes para produzir saída ou aguardar cancelamento; não executam comandos gerados pelo modelo e não usam login de fornecedor.

O smoke seguinte deve usar a janela Tauri: sessão pessoal Hermes com o modelo FreeLLMAPI configurado, pedido de ação local, conferir argumentos/caminho no cartão, negar uma chamada, permitir outra, cancelar consulta/execução e confirmar o resultado observado. Para o canal Codex, qualificar a versão CLI antes. Esse smoke é uma pendência de homologação, não uma etapa já aprovada.

## Complemento: Hermes/FreeLLMAPI como consumidor do runtime

A primeira integração estava limitada ao Codex; após a correção de escopo do usuário, o mesmo runtime foi ligado ao caminho Hermes/FreeLLMAPI.

- Ponte MCP publica as oito ferramentas novas com os schemas do registro Rust, além das oito anteriores.
- Catálogo esperado Rust, bootstrap Python e probe conferem exatamente dezesseis ferramentas efetivas `mcp__coucou__*`; terminal, ferramentas nativas e MCP estrangeiro seguem recusados.
- `BrokerIo` solicita aprovação única pelo controle existente, com `conversation_scope: false`. O executor compartilhado aceita o contexto pessoal Hermes, sem depender da versão/instalação Codex.
- Auxiliares delegados continuam sem habilitação para as novas ações locais.
- Interface informa disponibilidade no personagem Hermes, incluindo FreeLLMAPI.
- Erro de execução CLI mantém `isError: true` no retorno MCP, com resultado estruturado.

O teste do broker confirma equivalência de schemas, recusa de argumentos livres e replay; o teste Windows do runtime confirma consulta nativa de processos pelo contexto Hermes somente após aprovação simulada.

| Verificação após integração Hermes/FreeLLMAPI | Resultado |
| --- | --- |
| `cargo test --workspace` | 174 aprovados, zero falhas, três opt-in ignorados. |
| `cargo clippy --workspace --all-targets -- -D warnings` | Aprovado. |
| `npm test` | 124 aprovados, zero falhas, um opt-in ignorado; o opt-in do roteador foi executado separadamente abaixo. |
| `npx tsc --noEmit` e `npx vite build` | Aprovados. |
| `test-hermes-bootstrap.py` | 14 aprovados, incluindo catálogo ampliado e política de tool calling. |
| `managed-free-models.integration.test.mjs` com `COUCOU_TEST_FREE_RUNTIME` | Um teste aprovado com roteador real fixado e fornecedor interceptado. Preservou `tools`, `tool_choice`, chamada estruturada e mensagem `role: tool`. Seis chamadas ao fornecedor de fixture, zero ao fornecedor proibido e cancelamento observado. |
| `cargo build -p coucou --bin coucou` | Aprovado; executável atualizado somente em `windows/target/debug/`. Build levou 19 min 41 s nesta rodada. |
| Probe Hermes ACP/MCP com executável atualizado | Aprovado: atestação e quatro requisições de modelo com exatamente dezesseis ferramentas permitidas. Turno normal terminou com três requisições de modelo e duas de broker (`context_read`, `query_processes`). Tentativa de terminal nativo fez somente uma requisição adicional de modelo; não chegou ao broker. Sessão que falhou recusou novo prompt antes do modelo. |

Evidência do roteador: [`qualification.json`](../windows/target/managed-free-qualification-mLRRwu/qualification.json), com `toolCallingQualified: true` e `passed: true`. Uma primeira rodada expirou por timeout; a repetição com identificação das etapas passou. A causa do timeout inicial não foi comprovada.

Evidência ACP/MCP: [`evidence.json`](../windows/target/hermes-agent-runtime-probe-20261009/evidence.json), com `passed: true`, saída do runtime zero e nenhuma falha do fornecedor de fixture. A inicialização levou 127,513 s e a criação da sessão 209,711 s nesta máquina; o probe usa limite de 300 s por pedido. Este PASS comprova contrato e isolamento, não qualifica latência de uso na janela Tauri. O script agora identifica etapas no stdout para diagnosticar futuras rodadas lentas.

Reprodução adicional, a partir de `windows/`, usando os runtimes já qualificados no checkout (não instala serviços nem altera perfis globais):

```powershell
& './target/hermes-runtime-qualified-781334e/source/.venv/Scripts/python.exe' -I scripts/managed-services/test-hermes-bootstrap.py
$env:COUCOU_TEST_FREE_RUNTIME = 'D:/coucou/windows/target/validation-free-models/runtime-a6b2158c7c36ce19f888d0411846a1a0f2aa3f06'
node --import ./scripts/test-register.mjs --test tests/managed-free-models.integration.test.mjs
Remove-Item Env:COUCOU_TEST_FREE_RUNTIME
cargo build -p coucou --bin coucou
# O diretório de saída precisa ser novo em cada execução.
& './target/hermes-runtime-qualified-781334e/source/.venv/Scripts/python.exe' -I scripts/managed-services/probe-hermes-runtime.py --runtime ./target/hermes-runtime-qualified-781334e --coucou ./target/debug/coucou.exe --output ./target/hermes-agent-runtime-probe-nova-rodada
```

O probe usa Hermes/Python e transporte Rust reais, mas modelo e broker locais de fixture. Ele não comprova clique humano ou inferência Groq/Cerebras. O teste do roteador usa o FreeLLMAPI fixado real com todas as requisições de provedor interceptadas; não acessa contas remotas, e o resultado de processos nessa cadeia é fictício. A consulta nativa real é exercitada separadamente pelo teste Rust.
