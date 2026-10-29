[CmdletBinding()]
param(
    [string]$OutputDirectory = 'D:\Apps',
    [switch]$NoDeploy,
    [switch]$Launch
)

$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
Push-Location -LiteralPath $projectRoot
try {
    $devLoader = 'D:\Workshop\_codex\dev-env-20261026\Enter-DevEnv.ps1'
    if (-not (Get-Command link.exe -ErrorAction SilentlyContinue) -and
        (Test-Path -LiteralPath $devLoader)) {
        . $devLoader
    }
    if (-not (Test-Path -LiteralPath (Join-Path $projectRoot 'node_modules'))) {
        & npm ci --no-audit --no-fund
        if ($LASTEXITCODE -ne 0) { throw 'Frontend dependency installation failed.' }
    }
    & node '.\node_modules\@tauri-apps\cli\tauri.js' build --no-bundle -- --locked -j 1
    if ($LASTEXITCODE -ne 0) { throw 'Tauri release build failed.' }
    $artifact = Join-Path $projectRoot 'src-tauri\target\release\fishcreen.exe'
    if (-not (Test-Path -LiteralPath $artifact)) { throw "Missing artifact: $artifact" }
    $destination = $artifact
    if (-not $NoDeploy) {
        $resolvedOutput = [IO.Path]::GetFullPath($OutputDirectory)
        New-Item -ItemType Directory -Path $resolvedOutput -Force | Out-Null
        $destination = Join-Path $resolvedOutput 'fishcreen.exe'
        $running = @(Get-Process -Name fishcreen -ErrorAction SilentlyContinue |
            Where-Object { $_.Path -eq $destination })
        if ($running.Count -gt 0) {
            throw "fishcreen is running at $destination. Exit it before replacing the executable."
        }
        if (Test-Path -LiteralPath $destination) {
            $stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
            Copy-Item -LiteralPath $destination -Destination "$destination.$stamp.bak"
        }
        Copy-Item -LiteralPath $artifact -Destination $destination -Force
        $sourceHash = (Get-FileHash -LiteralPath $artifact -Algorithm SHA256).Hash
        $deployedHash = (Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash
        if ($sourceHash -ne $deployedHash) { throw 'Deployed executable hash mismatch.' }
    }
    Get-Item -LiteralPath $destination | Select-Object FullName,Length,LastWriteTime
    if ($Launch) { Start-Process -FilePath $destination -ArgumentList '--settings' -WindowStyle Hidden }
}
finally {
    Pop-Location
}
