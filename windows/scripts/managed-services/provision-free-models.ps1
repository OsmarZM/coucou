param([Parameter(Mandatory=$true)][string]$Destination)
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
Add-Type -AssemblyName System.IO.Compression.FileSystem
# Explicit setup only. Never invoke downloads or npm at Windows login.
$revision = 'a6b2158c7c36ce19f888d0411846a1a0f2aa3f06'
$sourceHash = 'ed7d1d8dbb8a41c475dbca2127a7844225880e3cf8ec6068f909769cb869d1c5'
$lockHash = '62d88845da3a91607622c9dcf745d8eed2cf1ed292e955151387858257edf1cc'
$nodeVersion = '22.22.0'
$nodeZipHash = 'c97fa376d2becdc8863fcd3ca2dd9a83a9f3468ee7ccf7a6d076ec66a645c77a'
$nodeExeHash = 'bae898add4643fcf890a83ad8ae56e20dce7e781cab161a53991ceba70c99ffb'
if (-not [Environment]::Is64BitOperatingSystem -or $env:PROCESSOR_ARCHITECTURE -eq 'ARM64') { throw 'Runtime qualificado somente para Windows x64.' }
$target = [IO.Path]::GetFullPath($Destination)
if (Test-Path -LiteralPath $target) { throw 'O runtime já existe. Não será sobrescrito automaticamente.' }
$parent = [IO.Path]::GetDirectoryName($target)
New-Item -ItemType Directory -Path $parent -Force | Out-Null
$stage = Join-Path $parent ('setup-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $stage | Out-Null
$account = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
& icacls.exe $stage /inheritance:r /grant:r ('*' + $account + ':(OI)(CI)F') '*S-1-5-18:(OI)(CI)F' | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'Não foi possível proteger o diretório do runtime.' }
function Assert-Hash([string]$Path,[string]$Expected) {
  if ((Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant() -ne $Expected) { throw 'A integridade do pacote não corresponde à versão fixada.' }
}
try {
  $archive = Join-Path $stage 'source.zip'
  Invoke-WebRequest -UseBasicParsing -TimeoutSec 120 -Uri ('https://codeload.github.com/tashfeenahmed/freellmapi/zip/' + $revision) -OutFile $archive
  Assert-Hash $archive $sourceHash
  [IO.Compression.ZipFile]::ExtractToDirectory($archive,$stage)
  [IO.Directory]::Move((Join-Path $stage ('freellmapi-' + $revision)),(Join-Path $stage 'source'))
  Assert-Hash (Join-Path $stage 'source\package-lock.json') $lockHash
  # Export the pinned async route without changing its provider logic. Coucou
  # must await its settlement; HTTP close alone is not completion evidence.
  $proxyPath = Join-Path $stage 'source\server\src\routes\proxy.ts'
  $proxyText = [IO.File]::ReadAllText($proxyPath)
  $routeStart = "proxyRouter.post('/chat/completions', async (req: Request, res: Response) => {"
  $routeEnd = "});`n`n// logRequest moved to lib/request-log.ts"
  if (($proxyText.Split(@($routeStart),[StringSplitOptions]::None).Length -ne 2) -or ($proxyText.Split(@($routeEnd),[StringSplitOptions]::None).Length -ne 2)) { throw 'A ponte de cancelamento não corresponde à fonte fixada.' }
  $proxyText = $proxyText.Replace($routeStart,"export const managedChatCompletionHandler = async (req: Request, res: Response) => {")
  $proxyText = $proxyText.Replace($routeEnd,"};`nproxyRouter.post('/chat/completions', managedChatCompletionHandler);`n`n// logRequest moved to lib/request-log.ts")
  [IO.File]::WriteAllText($proxyPath,$proxyText,[Text.UTF8Encoding]::new($false))
  $nodeArchive = Join-Path $stage 'node.zip'
  Invoke-WebRequest -UseBasicParsing -TimeoutSec 120 -Uri ('https://nodejs.org/dist/v' + $nodeVersion + '/node-v' + $nodeVersion + '-win-x64.zip') -OutFile $nodeArchive
  Assert-Hash $nodeArchive $nodeZipHash
  [IO.Compression.ZipFile]::ExtractToDirectory($nodeArchive,$stage)
  [IO.Directory]::Move((Join-Path $stage ('node-v' + $nodeVersion + '-win-x64')),(Join-Path $stage 'node'))
  $node = Join-Path $stage 'node\node.exe'
  Assert-Hash $node $nodeExeHash
  $npm = Join-Path $stage 'node\node_modules\npm\bin\npm-cli.js'
  $npmConfig = Join-Path $stage 'npm-private'
  New-Item -ItemType Directory -Path $npmConfig | Out-Null
  [IO.File]::WriteAllText((Join-Path $npmConfig 'user.npmrc'),'')
  [IO.File]::WriteAllText((Join-Path $npmConfig 'global.npmrc'),'')
  $env:NPM_CONFIG_USERCONFIG = Join-Path $npmConfig 'user.npmrc'
  $env:NPM_CONFIG_GLOBALCONFIG = Join-Path $npmConfig 'global.npmrc'
  $env:NPM_CONFIG_CACHE = Join-Path $npmConfig 'cache'
  $env:NODE_OPTIONS = $null
  Push-Location (Join-Path $stage 'source')
  try {
    # Runtime uses the upstream supported node:sqlite factory. No native
    # better-sqlite installer, remote lifecycle scripts, desktop or global npm.
    & $node $npm ci --ignore-scripts --workspace=server --workspace=shared --include-workspace-root=false --no-audit --no-fund
    if ($LASTEXITCODE -ne 0) { throw 'Falha ao instalar dependências fixadas.' }
    & $node $npm run build --workspace=server
    if ($LASTEXITCODE -ne 0) { throw 'Falha ao compilar o servidor fixado.' }
  } finally { Pop-Location }
  Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'free-models-server.mjs') -Destination (Join-Path $stage 'free-models-server.mjs')
  $files = [Collections.Generic.List[object]]::new()
  foreach ($directory in @('node','source\server\dist','source\node_modules')) {
    foreach ($file in (Get-ChildItem -LiteralPath (Join-Path $stage $directory) -Recurse -File)) {
      if ($file.Attributes -band [IO.FileAttributes]::ReparsePoint) { continue }
      $relative = $file.FullName.Substring($stage.Length + 1).Replace('\','/')
      $files.Add(@{ path=$relative; sha256=(Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant() })
    }
  }
  $wrapper = Join-Path $stage 'free-models-server.mjs'
  $files.Add(@{ path='free-models-server.mjs'; sha256=(Get-FileHash -LiteralPath $wrapper -Algorithm SHA256).Hash.ToLowerInvariant() })
  @{ schema=1; revision=$revision; nodeVersion=$nodeVersion; files=$files } | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $stage 'manifest.json') -Encoding UTF8
  # Source archives contain no credentials; remove only known staging files.
  Remove-Item -LiteralPath $archive,$nodeArchive
  # The two absolute paths share the explicitly requested parent. Directory
  # Move fails if a racing installer created the target; it never nests/merges.
  if ([IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($stage)) -ne $parent -or [IO.Path]::GetDirectoryName([IO.Path]::GetFullPath($target)) -ne $parent) { throw 'Destino de runtime fora do diretório explícito.' }
  [IO.Directory]::Move($stage,$target)
  Write-Output 'Runtime local provisionado. Nenhum provedor foi consultado ou serviço ativado.'
} catch {
  # Keep failed staging for diagnosis rather than a computed recursive delete.
  throw 'O provisionamento falhou. Nenhum serviço foi iniciado; o diretório de diagnóstico foi preservado.'
}
