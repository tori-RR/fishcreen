[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$source = Join-Path $projectRoot 'assets\fishcreen.svg'
$generated = Join-Path $projectRoot 'src-tauri\target\svg-icon-build'
$tauriCli = Join-Path $projectRoot 'node_modules\@tauri-apps\cli\tauri.js'

if (-not (Test-Path -LiteralPath $tauriCli)) {
    throw 'Install project dependencies with npm ci before generating icons.'
}

# Keep the SVG as the source of truth; the Windows runtime consumes its ICO export.
& node $tauriCli icon $source --output $generated
if ($LASTEXITCODE -ne 0) { throw 'Icon generation failed.' }

foreach ($extension in @('png', 'ico')) {
    Copy-Item -LiteralPath (Join-Path $generated "icon.$extension") `
        -Destination (Join-Path $projectRoot "assets\fishcreen.$extension") -Force
}
Get-Item -LiteralPath $source, (Join-Path $projectRoot 'assets\fishcreen.png'), `
    (Join-Path $projectRoot 'assets\fishcreen.ico') | Select-Object FullName, Length
