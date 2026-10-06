# Abre o aplicativo compilado sem substituir os executáveis antigos da raiz.
$ErrorActionPreference = 'Stop'
$taskAppPath = Join-Path $PSScriptRoot 'windows\target\release\coucou.exe'
if (-not (Test-Path -LiteralPath $taskAppPath -PathType Leaf)) {
    throw 'Compile primeiro: entre na pasta windows e execute npm ci e npm run pack.'
}
$taskRunningApps = @(Get-Process -Name coucou -ErrorAction SilentlyContinue)
foreach ($taskRunningApp in $taskRunningApps) {
    if (-not $taskRunningApp.Path -or $taskRunningApp.Path -ne $taskAppPath) {
        throw 'Outra cópia do Coucou está aberta. Saia pelo ícone da bandeja antes de abrir a versão atual. Este script não encerra agentes nem aplicativos automaticamente.'
    }
}
Start-Process -FilePath $taskAppPath -WorkingDirectory (Split-Path -Parent $taskAppPath) -WindowStyle Hidden
