param([Parameter(Mandatory=$true)][string]$Destination,[string]$SourceArchive)
$ErrorActionPreference = 'Stop'
# Explicit setup only. No setup, update, model request or global hook at login.
$revision = '781334eea4b9225a3e194faf0c241d9afe218634'
$sourceHash = 'ab1dabd99ab50981f4d6b89c5c9288627b24066152bc58d3f6940628c4f23bed'
$lockHash = '2bc4db483e9cba927cd7147f9ccbb09635f85bb3f1f36d6e9d51d06b65d1c67e'
$pmLockHash = 'b096ea27caccf2cfb4c63f36b7e77acd9d540dc1af421fdbce148302ebb949d4'
$uvVersion = '0.9.26'
$uvZipHash = 'eb02fd95d8e0eed462b4a67ecdd320d865b38c560bffcda9a0b87ec944bdf036'
$pythonVersion = '3.14.2'
if (-not [Environment]::Is64BitOperatingSystem -or $env:PROCESSOR_ARCHITECTURE -eq 'ARM64') { throw 'Runtime Hermes qualificado somente para Windows x64.' }
$target = [IO.Path]::GetFullPath($Destination)
if (-not [IO.Path]::IsPathRooted($target) -or $target -eq [IO.Path]::GetPathRoot($target)) { throw 'Escolha uma pasta exclusiva para o runtime Hermes.' }
if (Test-Path -LiteralPath $target) { throw 'O runtime já existe e foi preservado. Use outro destino para uma instalação explícita.' }
$parent = [IO.Path]::GetDirectoryName($target)
New-Item -ItemType Directory -Path $parent -Force | Out-Null
for ($probe = $parent; $probe; $probe = [IO.Path]::GetDirectoryName($probe)) {
  if ((Get-Item -LiteralPath $probe).Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'O destino contém link/reparse não permitido.' }
}
$stage = Join-Path $parent ('hermes-setup-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $stage | Out-Null
$account = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
& icacls.exe $stage /inheritance:r /grant:r ('*' + $account + ':(OI)(CI)F') '*S-1-5-18:(OI)(CI)F' | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'Não foi possível proteger o runtime Hermes.' }
function Assert-Hash([string]$Path,[string]$Expected) {
  if ((Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant() -ne $Expected) { throw 'A integridade do pacote Hermes não corresponde à revisão fixada.' }
}
function Invoke-PrivateProcess([string]$Executable,[string[]]$Arguments,[hashtable]$Environment,[string]$Log) {
  $start = New-Object Diagnostics.ProcessStartInfo
  $start.FileName = $Executable
  $start.UseShellExecute = $false
  $start.CreateNoWindow = $true
  $start.RedirectStandardOutput = $true
  $start.RedirectStandardError = $true
  # ProcessStartInfo.Arguments uses Win32 quoting; never pass through a shell.
  $start.Arguments = (($Arguments | ForEach-Object {
    '"' + [regex]::Replace([regex]::Replace($_,'(\\*)"','$1$1\"'),'(\\+)$','$1$1') + '"'
  }) -join ' ')
  $start.EnvironmentVariables.Clear()
  foreach ($name in $Environment.Keys) { $start.EnvironmentVariables[$name] = $Environment[$name] }
  $process = New-Object Diagnostics.Process
  $process.StartInfo = $start
  try {
    if (-not $process.Start()) { throw 'Não foi possível iniciar o processo privado.' }
    $stdout = $process.StandardOutput.ReadToEndAsync()
    $stderr = $process.StandardError.ReadToEndAsync()
    $process.WaitForExit()
    $content = $stdout.GetAwaiter().GetResult() + $stderr.GetAwaiter().GetResult()
    [IO.File]::WriteAllText($Log,$content,(New-Object Text.UTF8Encoding($false)))
    if ($process.ExitCode -ne 0) { throw 'O processo privado falhou; consulte o log de instalação.' }
  } finally { $process.Dispose() }
}
Add-Type -AssemblyName System.IO.Compression.FileSystem
function Expand-RuntimeSource([string]$Archive,[string]$Staging) {
  # The upstream archive also includes 14,000 desktop, website, contributor and
  # test files. Install the server runtime closure, keeping the fixed source
  # archive hash and lockfiles; never install another desktop application.
  $runtimeDirectories = @('acp_adapter','agent','hermes_cli','pm','tools','gateway','plugins','cron','providers','hermes_platform','locales','tui_gateway')
  $zip = [IO.Compression.ZipFile]::OpenRead($Archive)
  try {
    foreach ($entry in $zip.Entries) {
      $parts = $entry.FullName.Split('/')
      if ($parts.Length -lt 2 -or -not $parts[1]) { continue }
      $rootFile = $parts.Length -eq 2
      if (-not $rootFile -and $parts[1] -notin $runtimeDirectories) { continue }
      if ($entry.FullName.EndsWith('/')) { continue }
      $output = [IO.Path]::GetFullPath((Join-Path $Staging $entry.FullName.Replace('/',[IO.Path]::DirectorySeparatorChar)))
      if (-not $output.StartsWith($Staging + [IO.Path]::DirectorySeparatorChar)) { throw 'Arquivo fora do staging recusado.' }
      [IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($output)) | Out-Null
      [IO.Compression.ZipFileExtensions]::ExtractToFile($entry,$output,$false)
    }
  } finally { $zip.Dispose() }
}
try {
  $archive = Join-Path $stage 'source.zip'
  if ($SourceArchive) { Copy-Item -LiteralPath $SourceArchive -Destination $archive }
  else { Invoke-WebRequest -UseBasicParsing -Uri ('https://codeload.github.com/NousResearch/hermes-agent/zip/' + $revision) -OutFile $archive -TimeoutSec 120 }
  Assert-Hash $archive $sourceHash
  Expand-RuntimeSource $archive $stage
  $sourceOriginal = Join-Path $stage ('hermes-agent-' + $revision)
  # Both move targets are verified beneath the explicit staging directory.
  if (-not ([IO.Path]::GetFullPath($sourceOriginal).StartsWith($stage + [IO.Path]::DirectorySeparatorChar))) { throw 'Origem do pacote fora do staging.' }
  Move-Item -LiteralPath $sourceOriginal -Destination (Join-Path $stage 'source')
  Assert-Hash (Join-Path $stage 'source\uv.lock') $lockHash
  Assert-Hash (Join-Path $stage 'source\pm\lock.json') $pmLockHash
  $uvArchive = Join-Path $stage 'uv.zip'
  Invoke-WebRequest -UseBasicParsing -Uri ('https://github.com/astral-sh/uv/releases/download/' + $uvVersion + '/uv-x86_64-pc-windows-msvc.zip') -OutFile $uvArchive -TimeoutSec 120
  Assert-Hash $uvArchive $uvZipHash
  [IO.Compression.ZipFile]::ExtractToDirectory($uvArchive, (Join-Path $stage 'uv'))
  Remove-Item -LiteralPath $archive,$uvArchive
  $resolvedStage = [IO.Path]::GetFullPath((Get-Item -LiteralPath $stage).FullName)
  if (-not $resolvedStage.StartsWith($parent + [IO.Path]::DirectorySeparatorChar) -or [IO.Path]::GetFullPath($target) -ne $target) { throw 'Movimento do runtime fora do destino autorizado.' }
  # Install dependencies only after the source reaches its final path: Windows
  # venvs reference an absolute Python path and must not be moved after setup.
  Move-Item -LiteralPath $resolvedStage -Destination $target
  $uv = Join-Path $target 'uv\uv.exe'
  if (-not (Test-Path -LiteralPath $uv)) { throw 'Executável uv fixado indisponível.' }
  $source = Join-Path $target 'source'
  $isolatedEnv = @{
    UV_PYTHON_INSTALL_DIR=(Join-Path $target 'python'); UV_CACHE_DIR=(Join-Path $target 'cache');
    UV_NO_CONFIG='1'; UV_PYTHON_PREFERENCE='only-managed'; UV_PYTHON_INSTALL_BIN='0';
    HERMES_HOME=(Join-Path $target 'setup-home'); HERMES_DISABLE_LAZY_INSTALLS='1';
    HOME=(Join-Path $target 'setup-home'); USERPROFILE=(Join-Path $target 'setup-home');
    APPDATA=(Join-Path $target 'setup-appdata'); LOCALAPPDATA=(Join-Path $target 'setup-localappdata')
  }
  foreach ($name in @('SystemRoot','WINDIR','SYSTEMDRIVE','NUMBER_OF_PROCESSORS','PROCESSOR_ARCHITECTURE')) {
    $value = [Environment]::GetEnvironmentVariable($name,'Process')
    if ($value) { $isolatedEnv[$name] = $value }
  }
  $isolatedEnv['PATH'] = Join-Path $isolatedEnv['SystemRoot'] 'System32'
  # The fixed uv binary contains the download hash for this Python build.
  # --no-build disallows dependency source/build hooks; frozen lock selects
  # pinned wheels. No user index credentials, proxy override or Python/uv
  # config environment can reach these child processes.
  Invoke-PrivateProcess $uv @('python','install',$pythonVersion,'--no-bin') $isolatedEnv (Join-Path $target 'setup-python.log')
  Invoke-PrivateProcess $uv @('sync','--project',$source,'--python',$pythonVersion,'--frozen','--no-dev','--no-install-project','--no-build','--extra','acp','--extra','mcp') $isolatedEnv (Join-Path $target 'setup-dependencies.log')
  $python = Join-Path $source '.venv\Scripts\python.exe'
  Invoke-PrivateProcess $python @('-I','-c','import acp, openai, mcp, dotenv; print("dependencies-ready")') $isolatedEnv (Join-Path $target 'setup-check.log')
  $result = @{ schemaVersion=1; owner='coucou'; revision=$revision; uvVersion=$uvVersion; pythonVersion=$pythonVersion;
    sourceArchiveSha256=$sourceHash; lockSha256=$lockHash; pythonExecutable=$python; sourceDirectory=$source }
  [IO.File]::WriteAllText((Join-Path $target 'coucou-runtime.json'),($result | ConvertTo-Json -Depth 4),(New-Object Text.UTF8Encoding($false)))
  $result | ConvertTo-Json -Depth 4 -Compress | Write-Output
} catch {
  # Keep only owned partial installation/logs for diagnosis. Never recursively
  # remove a computed directory or overwrite another Hermes installation.
  throw 'O provisionamento Hermes falhou. Logs e instalação parcial foram preservados; nenhuma chave, modelo, tarefa ou configuração global foi utilizada.'
}
