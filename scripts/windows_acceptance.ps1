#Requires -Version 5.1
<#
  Windows real-machine acceptance evidence collector for QA-03 / DESKTOP-ACCEPT-ICA-R1.

  This script DOES NOT perform installation, upgrade, uninstall or crash-recovery
  steps by itself. It records the machine facts, checks the prerequisites that can
  be verified without changing the host, and writes a structured evidence bundle.
  Every product scenario W2..W8 stays MANUAL until a human executes it and signs.

  Usage (PowerShell 5.1+, no elevated privileges required):
    powershell -NoProfile -ExecutionPolicy Bypass -File scripts/windows_acceptance.ps1
    powershell -NoProfile -ExecutionPolicy Bypass -File scripts/windows_acceptance.ps1 `
      -InstallerPath 'C:\path\ICA-Personal-0.2.2-x64-setup.exe' `
      -DataDir 'C:\Temp\ica-accept-data' `
      -OutDir 'docs/testing/evidence/windows-acceptance'
#>
[CmdletBinding()]
param(
  [string] $InstallerPath = '',
  [string] $DataDir = '',
  [string] $OutDir = ''
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Write-Utf8NoBom([string] $Path, [string] $Text) {
  $parent = Split-Path -Parent $Path
  if (-not (Test-Path -LiteralPath $parent)) { New-Item -ItemType Directory -Path $parent -Force | Out-Null }
  $encoding = New-Object System.Text.UTF8Encoding($false)
  [System.IO.File]::WriteAllText($Path, ($Text -replace "`r`n", "`n"), $encoding)
}

function Get-GitFact([string] $RepoRoot, [string[]] $GitArgs) {
  try {
    $output = & git -C $RepoRoot @GitArgs 2>$null
    if ($LASTEXITCODE -ne 0) { return '' }
    return ($output -join '').Trim()
  } catch {
    return ''
  }
}

function Get-DirectoryState([string] $Path) {
  if (-not (Test-Path -LiteralPath $Path)) {
    return @{ exists = $false; files = 0; placeholder = $false }
  }
  $files = @(Get-ChildItem -LiteralPath $Path -Recurse -File -ErrorAction SilentlyContinue)
  $placeholder = ($files.Count -le 2 -and @($files | Where-Object { $_.Name -eq 'README.md' }).Count -ge 1)
  return @{ exists = $true; files = $files.Count; placeholder = $placeholder }
}

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
if (-not $OutDir) { $OutDir = Join-Path $repoRoot 'docs/testing/evidence/windows-acceptance' }
if (-not [System.IO.Path]::IsPathRooted($OutDir)) { $OutDir = Join-Path $repoRoot $OutDir }
if (-not $DataDir) { $DataDir = $env:ICA_DATA_DIR }
if (-not $DataDir) { $DataDir = Join-Path $env:LOCALAPPDATA 'ICA\data' }

$stamp = (Get-Date).ToUniversalTime().ToString('yyyyMMddTHHmmssZ')
$jsonPath = Join-Path $OutDir "windows-acceptance-$stamp.json"
$mdPath = Join-Path $OutDir "windows-acceptance-$stamp.md"
$head = Get-GitFact $repoRoot @('rev-parse', 'HEAD')
$branch = Get-GitFact $repoRoot @('rev-parse', '--abbrev-ref', 'HEAD')

$os = Get-CimInstance Win32_OperatingSystem
$environment = [ordered]@{
  captured_at_utc = (Get-Date).ToUniversalTime().ToString('o')
  machine = $env:COMPUTERNAME
  user = $env:USERNAME
  os_caption = $os.Caption
  os_version = $os.Version
  os_build = $os.BuildNumber
  os_architecture = $os.OSArchitecture
  powershell_version = $PSVersionTable.PSVersion.ToString()
  process_architecture = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
  git_head = $head
  git_branch = $branch
  data_dir = $DataDir
}

$installer = $null
if ($InstallerPath) {
  if (-not (Test-Path -LiteralPath $InstallerPath -PathType Leaf)) {
    throw "InstallerPath does not exist: $InstallerPath"
  }
  $item = Get-Item -LiteralPath $InstallerPath
  $installer = [ordered]@{
    path = $item.FullName
    name = $item.Name
    bytes = $item.Length
    sha256 = (Get-FileHash -LiteralPath $item.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
  }
}

$bundle = Join-Path $repoRoot 'desktop/tauri-host/bundle-resources'
$resources = [ordered]@{
  sidecar = Get-DirectoryState (Join-Path $bundle 'sidecar')
  ui = Get-DirectoryState (Join-Path $bundle 'ui')
  browsers = Get-DirectoryState (Join-Path $bundle 'browsers')
  version_manifest = (Test-Path -LiteralPath (Join-Path $bundle 'version-manifest.json'))
  sha256sums = (Test-Path -LiteralPath (Join-Path $bundle 'SHA256SUMS'))
}

$checks = New-Object System.Collections.Generic.List[object]
function Add-Check([string] $Id, [string] $Title, [string] $Status, [string] $Detail, [string[]] $Evidence) {
  $checks.Add([ordered]@{
    id = $Id
    title = $Title
    status = $Status
    detail = $Detail
    evidence = @($Evidence)
  })
}

Add-Check 'W1' 'Windows clean install' $(if ($installer) { 'READY' } else { 'BLOCKED' }) `
  $(if ($installer) { "Installer captured: $($installer.name). Silent and interactive installs still require a human." } else { 'No installer supplied; nothing was installed.' }) `
  $(if ($installer) { @($installer.path) } else { @() })

Add-Check 'W2' 'First launch / health' 'MANUAL' `
  'Launch the installed app on this host; record window appearance, health probe output and crash dialogs.' @()
Add-Check 'W3' 'SQLite schema auto-initialization' 'MANUAL' `
  'Inspect the data dir DDL against crates/ica-infra/schema/ica-baseline.sql (32 tables / 121 indexes).' @()
Add-Check 'W4' 'In-place upgrade keeps data' 'MANUAL' `
  'Install previous version, write business rows, install current version, compare row counts.' @()
Add-Check 'W5' 'Migration idempotence' 'MANUAL' `
  'Check schema_migrations and row snapshots before/after upgrade.' @()
Add-Check 'W6' 'Backup/restore via product path' 'MANUAL' `
  'Mechanism-level drill already exists (TEST-ICA-DRILL-001); product UI/NSIS path still needs real evidence.' @()
Add-Check 'W7' 'Restart after forced termination' 'MANUAL' `
  'taskkill /F the app, restart, verify probe data, backup list and audit trail persisted.' @()
Add-Check 'W8' 'Uninstall keeps user data' 'MANUAL' `
  'Compare registry keys and data dir tree before/after uninstall; native messaging keys must disappear.' @()

$w9Status = 'BLOCKED'
$w9Detail = 'Installer version traceability cannot be checked without an installer.'
$w9Evidence = @()
if ($installer) {
  $tauriConfig = Get-Content -LiteralPath (Join-Path $repoRoot 'desktop/tauri-host/tauri.conf.json') -Raw | ConvertFrom-Json
  $declaredVersion = [string] $tauriConfig.version
  $installerHasVersion = $installer.name -match [regex]::Escape($declaredVersion)
  $w9Evidence = @($installer.path, (Join-Path $repoRoot 'desktop/tauri-host/tauri.conf.json'))
  if ($installerHasVersion) {
    $w9Status = 'READY'
    $w9Detail = "Installer filename contains tauri.conf.json version $declaredVersion; signature, version-manifest and SHA256SUMS still need human verification."
  } else {
    $w9Status = 'FAIL'
    $w9Detail = "Installer name does not contain tauri.conf.json version $declaredVersion."
  }
}
Add-Check 'W9' 'Version / build traceability' $w9Status $w9Detail $w9Evidence

$blocking = @($resources.sidecar.placeholder, $resources.ui.placeholder, $resources.browsers.placeholder) -contains $true
$releaseReadiness = [ordered]@{
  installer_present = [bool] $installer
  bundle_resources_placeholder = $blocking
  note = 'Bundle resources must be staged from a real sidecar/UI/browser runtime before an NSIS installer can be built.'
}

$report = [ordered]@{
  schema = 'DESKTOP-ACCEPT-ICA-R1/preflight-v1'
  kind = 'PREFLIGHT_NOT_ACCEPTANCE'
  environment = $environment
  installer = $installer
  bundle_resources = $resources
  release_readiness = $releaseReadiness
  checks = $checks.ToArray()
  sign_off = [ordered]@{
    status = 'NOT_SIGNED'
    note = 'Agent must not sign Windows acceptance. A human owner signs W1..W9 in docs/testing/desktop-release-acceptance.md.'
  }
}

Write-Utf8NoBom $jsonPath ($report | ConvertTo-Json -Depth 12)

$lines = @()
$lines += '# Windows real-machine acceptance - preflight evidence'
$lines += ''
$lines += "> PREFLIGHT ONLY. Not an acceptance pass, not signed. Captured $($environment.captured_at_utc)."
$lines += ''
$lines += "- Host: $($environment.os_caption) $($environment.os_version) (build $($environment.os_build), $($environment.os_architecture))"
$lines += "- git HEAD: $head ($branch)"
$lines += "- Installer: $(if ($installer) { "$($installer.name) | sha256=$($installer.sha256)" } else { 'not supplied' })"
$lines += "- Bundle resources placeholder: $blocking"
$lines += ''
$lines += '| ID | Scenario | Status | Detail |'
$lines += '|---|---|---|---|'
foreach ($c in $checks.ToArray()) {
  $detail = $c.detail -replace '\|', '\|'
  $lines += "| $($c.id) | $($c.title) | $($c.status) | $detail |"
}
$lines += ''
$lines += "Machine-readable evidence: $(Split-Path -Leaf $jsonPath)"
$lines += ''
Write-Utf8NoBom $mdPath (($lines -join "`n") + "`n")

Write-Host "PREFLIGHT written: $jsonPath"
Write-Host "PREFLIGHT written: $mdPath"
$failures = @($checks.ToArray() | Where-Object { $_.status -eq 'FAIL' })
if ($failures.Count -gt 0) { exit 2 }
exit 0
