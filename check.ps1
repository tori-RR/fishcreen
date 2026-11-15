[CmdletBinding()]
param(
    [switch]$NoBuild,
    [string]$OutputDirectory
)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
Push-Location -LiteralPath $projectRoot
try {
    . (Join-Path $projectRoot 'scripts\frontend-deps.ps1')
    Ensure-FrontendDependencies -ProjectRoot $projectRoot
    foreach ($tool in @('npm.cmd', 'cargo', 'powershell.exe')) {
        if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) { throw "Required build tool is not on PATH: $tool" }
    }
    & npm.cmd run check
    if ($LASTEXITCODE -ne 0) { throw 'TypeScript check failed.' }
    & npm.cmd test
    if ($LASTEXITCODE -ne 0) { throw 'Frontend regression tests failed.' }
    & npm.cmd run test:build
    if ($LASTEXITCODE -ne 0) { throw 'Dependency policy tests failed.' }
    & cargo fmt --manifest-path src-tauri/Cargo.toml --check
    if ($LASTEXITCODE -ne 0) { throw 'Rust formatting check failed.' }
    & cargo test --manifest-path src-tauri/Cargo.toml --locked -j 1
    if ($LASTEXITCODE -ne 0) { throw 'Rust tests failed.' }
    & npm.cmd run test:ui
    if ($LASTEXITCODE -ne 0) { throw 'Chromium regression tests failed.' }
    if (-not $NoBuild) {
        if ([string]::IsNullOrWhiteSpace($OutputDirectory)) {
            $OutputDirectory = Join-Path 'D:\Apps' ('fishcreen-validation-' + (Get-Date -Format 'yyyyMMdd-HHmmss-fff'))
        }
        if (Test-Path -LiteralPath $OutputDirectory) { throw 'Validation output must be a new directory; refusing to overwrite it.' }
        & (Join-Path $projectRoot 'build.ps1') -OutputDirectory $OutputDirectory
        if ($LASTEXITCODE -ne 0) { throw 'Release-mode validation build failed.' }
    }
    Write-Host 'All requested checks passed. No installed program was replaced or launched.'
}
finally { Pop-Location }
