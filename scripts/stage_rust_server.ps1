#Requires -Version 5.1
<#
  Stage the Rust HTTP server binary into the Tauri host's bundle-resources.

  Embedded Mode starts ica-server on the first Rust-routed /api request; the
  host looks for it at <unit>/ica-server.exe (see rust_server_spec in
  desktop/tauri-host/src/main.rs). Without this step the installer ships a
  working Python sidecar but every business API request returns 404.

  Usage:
    powershell -NoProfile -ExecutionPolicy Bypass -File scripts/stage_rust_server.ps1
    powershell -NoProfile -ExecutionPolicy Bypass -File scripts/stage_rust_server.ps1 -SkipBuild
#>
[CmdletBinding()]
param(
  [string] $RepoRoot = '',
  [switch] $SkipBuild
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

if (-not $RepoRoot) { $RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path }
$RepoRoot = (Resolve-Path -LiteralPath $RepoRoot).Path.TrimEnd('\')
$exeName = if ($env:OS -eq 'Windows_NT') { 'ica-server.exe' } else { 'ica-server' }
$resources = Join-Path $RepoRoot 'desktop/tauri-host/bundle-resources'

if (-not $SkipBuild) {
  Write-Host "=== cargo build --release -p ica-server ==="
  & cargo build --release -p ica-server --manifest-path (Join-Path $RepoRoot 'Cargo.toml')
  if ($LASTEXITCODE -ne 0) { throw "ica-server build failed (exit $LASTEXITCODE)" }
}

$targetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $RepoRoot 'target' }
$source = Join-Path $targetDir "release/$exeName"
if (-not (Test-Path -LiteralPath $source -PathType Leaf)) {
  throw "built server binary not found: $source"
}
if (-not (Test-Path -LiteralPath $resources)) {
  New-Item -ItemType Directory -Path $resources -Force | Out-Null
}
$destination = Join-Path $resources $exeName
Copy-Item -LiteralPath $source -Destination $destination -Force
$item = Get-Item -LiteralPath $destination
Write-Host ("STAGED {0} bytes={1}" -f $destination, $item.Length)
