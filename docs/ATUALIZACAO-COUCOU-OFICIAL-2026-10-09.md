# Atualização do Coucou oficial no Windows

## Objetivo

Atualizar o aplicativo que já estava instalado na máquina com o pacote oficial do projeto Louis-CFM/coucou, preservando a instalação anterior, as configurações e o código local do Agent Runtime.

## Diagnóstico

- Aplicativo instalado: `D:\Nova pasta\Coucou\coucou.exe`, versão 0.2.0.
- A sessão anterior abriu `D:\coucou\windows\target\debug\coucou.exe`, versão local 0.1.4. Essa troca para uma build anterior foi inadequada para atualizar a experiência do aplicativo instalado.
- `D:\coucou` é um fork do Coucou oficial, com `origin` apontando para `https://github.com/Louis-CFM/coucou.git` e `fork` para `https://github.com/OsmarZM/coucou.git`; não é o repositório Agent Island.
- O checkout local permanece em `codex/multiagent-windows`, HEAD `682af47e46d3eb541334a093c22ceae58409a6c1`, com alterações anteriores preservadas.
- A consulta ao upstream atualizou `origin/main` para `c946650bec0e37fd902322c8f149b7e83f06d22f`. O alias local `windows-latest` não foi sobrescrito quando o fetch informou conflito de tag; a atualização usa a tag versionada `windows-v0.3.0`.

## Pacote e integridade

- Release oficial: https://github.com/Louis-CFM/coucou/releases/tag/windows-v0.3.0
- Commit da release: `637b576a7c5261f7fee11edf3fb75b1ab92c8ab6`.
- Instalador: `windows/release/official-0.3.0/Coucou-Windows-0.3.0-setup.exe`.
- SHA-256 conferido com o digest da API GitHub: `6ce61dfaf1521845d9385fe9be28ce13ad4b6b084bdb6d4d2376ca2872a1bb80`.
- Authenticode: `NotSigned`, conforme a publicação oficial. Não foi removida marca de origem nem alterada política de segurança do Windows.

## Preservação

Backup local: `C:\Users\FortaTech\AppData\Local\Coucou-update-backups\official-0.3.0-20261009-112421`.

Contém a instalação anterior 0.2.0, configurações Roaming, cópia dos dados locais selecionados (personal, documents, inbox, bin e recap), ZIP de 77 arquivos locais modificados/não rastreados e manifesto de origem. A cópia dos dados ocorreu com a build de desenvolvimento aberta; não é um backup transacional dos bancos SQLite. O armazenamento ativo não foi removido nem restaurado por cima da instalação atual.

Credenciais do Windows não foram exportadas, apagadas ou modificadas. Configurações globais de fornecedores e instalação de hooks não foram acionadas. O instalador atualiza o relay distribuído com o aplicativo; a instalação de hooks continua no fluxo de revisão do Coucou.

## Resultado da instalação

O instalador oficial foi executado no diretório existente `D:\Nova pasta\Coucou`.

- Versão do executável instalado: **0.3.0**.
- Registro de desinstalação HKCU: **0.3.0**, no mesmo diretório.
- SHA-256 do executável instalado: `113248c961aca894afda1fa0201074b8c2194c6fb2fb0e05d7a3531c71ad43e5`.
- SHA-256 do relay instalado: `f09ef5c8b72cef98347fdb4b6b2ca6bbe6f947c01378c5415926cccc3b17ad22`.
- SHA-256 de `settings.json` antes/depois da instalação: `77f89c47652147052cceac46939efed5fad8092b36613e10d3d9060fea4b776f`; arquivo preservado byte a byte.

A build de desenvolvimento ainda permanecia em execução após solicitar o fechamento normal da janela. A abertura da instância oficial depende da saída pelo menu da bandeja; instalação concluída não comprova que a janela oficial já iniciou.

## Limites e próximo ponto de integração

O pacote oficial 0.3.0 não inclui o Agent Runtime nem o gerenciamento Hermes/FreeLLMAPI desenvolvido neste fork. Esse código está preservado no checkout e no ZIP, mas sua integração na interface e nos contratos da base 0.3.0 continua pendente. `git fetch` atualiza referências; não equivale a merge/rebase da implementação local.

A comparação entre HEAD local e upstream envolve 194 arquivos Windows e mudanças amplas em chat, configuração, hooks, identidade, i18n, atalhos e interface. Não foi realizado merge automático que pudesse substituir silenciosamente as funcionalidades ou guardas locais.

Não foram alegados testes de fornecedor remoto, janela oficial, autorização humana, assinatura do pacote ou publicação de um fork atualizado. Esta evidência confirma origem, digest, backup e atualização da instalação oficial.
