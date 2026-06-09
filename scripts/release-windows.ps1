$ErrorActionPreference = 'Stop'

$repoRoot = Split-Path -Parent $PSScriptRoot
Set-Location $repoRoot

Write-Host 'Running full verification gate...'
pnpm verify

Write-Host 'Building Windows release artifacts...'
pnpm tauri build

$releaseDir = Join-Path $repoRoot 'src-tauri\target\release'
$portableExe = Join-Path $releaseDir 'tauri-app.exe'
$setupExe = Get-ChildItem -Path (Join-Path $releaseDir 'bundle\nsis') -Filter '*setup.exe' -ErrorAction SilentlyContinue | Select-Object -First 1

if (-not (Test-Path $portableExe)) {
  throw "Portable runner missing: $portableExe"
}

if (-not $setupExe) {
  throw 'NSIS setup exe not found under src-tauri\target\release\bundle\nsis'
}

$expectedResources = @(
  (Join-Path $releaseDir 'resources\playwright-bridge\index.mjs'),
  (Join-Path $releaseDir 'resources\node\node.exe'),
  (Join-Path $releaseDir 'resources\node_modules\playwright\package.json')
)

foreach ($resourcePath in $expectedResources) {
  if (-not (Test-Path $resourcePath)) {
    throw "Bundled runtime resource missing: $resourcePath"
  }
}

Write-Host "Portable runner: $portableExe"
Write-Host "Setup installer: $($setupExe.FullName)"
Write-Host 'Release artifacts verified.'
