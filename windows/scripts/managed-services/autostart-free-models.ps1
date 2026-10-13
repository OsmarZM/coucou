param([Parameter(Mandatory=$true)][string]$Executable,[Parameter(Mandatory=$true)][ValidateSet('enable','disable','status')][string]$Action)
$ErrorActionPreference = 'Stop'
$path = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
$name = 'CoucouFreeModels'
if (-not [IO.Path]::IsPathRooted($Executable) -or $Executable.Contains('"')) { throw 'Executável inválido.' }
$command = '"' + [IO.Path]::GetFullPath($Executable) + '" --managed-free-service'
$item = Get-ItemProperty -LiteralPath $path -Name $name -ErrorAction SilentlyContinue
$existing = if ($null -eq $item) { $null } else { $item.$name }
if ($Action -eq 'status') { if ($existing -eq $command) { Write-Output 'enabled' } elseif ($null -eq $existing) { Write-Output 'disabled' } else { Write-Output 'foreign' }; exit }
if ($null -ne $existing -and $existing -ne $command) { throw 'O registro de inicialização pertence a outra instalação e foi preservado.' }
if ($Action -eq 'enable') {
  if (-not (Test-Path -LiteralPath $Executable -PathType Leaf)) { throw 'Executável ausente.' }
  New-Item -Path $path -Force | Out-Null
  New-ItemProperty -LiteralPath $path -Name $name -Value $command -PropertyType String -Force | Out-Null
} elseif ($null -ne $existing) {
  Remove-ItemProperty -LiteralPath $path -Name $name
}
