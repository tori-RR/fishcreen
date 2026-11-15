$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'frontend-deps.ps1')

$suiteRoot = Join-Path ([IO.Path]::GetTempPath()) ('fishcreen-deps-test-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $suiteRoot | Out-Null
$script:assertions = 0
$script:scenarios = 0
$script:installs = 0
$testToolchain = @{ nodeMajor = '24'; npm = '11.0.0'; platform = 'win32'; architecture = 'x64' }

function Assert-DependencyTest {
    param([bool]$Condition, [string]$Message)
    $script:assertions++
    if (-not $Condition) { throw "Assertion failed: $Message" }
}
function Assert-DependencyThrows {
    param([scriptblock]$Run, [string]$Message)
    $failed = $false
    try { & $Run | Out-Null } catch { $failed = $true }
    Assert-DependencyTest $failed $Message
}
function Write-DependencyFixtureFile {
    param([string]$Path, [string]$Text)
    [IO.Directory]::CreateDirectory((Split-Path -Parent $Path)) | Out-Null
    [IO.File]::WriteAllText($Path, $Text, [Text.UTF8Encoding]::new($false))
}
function New-DependencyFixture {
    $script:scenarios++
    $root = Join-Path $suiteRoot "project-$script:scenarios"
    New-Item -ItemType Directory -Path $root | Out-Null
    $manifest = [ordered]@{ name = 'fixture'; version = '1.0.0'; dependencies = @{ alpha = '1.0.0' } }
    $lock = [ordered]@{ name = 'fixture'; version = '1.0.0'; lockfileVersion = 3; packages = @{ '' = $manifest; 'node_modules/alpha' = @{ version = '1.0.0' } } }
    Write-DependencyFixtureFile (Join-Path $root 'package.json') ($manifest | ConvertTo-Json -Depth 10)
    Write-DependencyFixtureFile (Join-Path $root 'package-lock.json') ($lock | ConvertTo-Json -Depth 10)
    return $root
}
function Install-DependencyFixture {
    param([string]$Root)
    $script:installs++
    Write-DependencyFixtureFile (Join-Path $Root 'node_modules/alpha/package.json') '{"name":"alpha","version":"1.0.0"}'
}
function Change-DependencyLockBytes {
    param([string]$Root)
    [IO.File]::AppendAllText((Join-Path $Root 'package-lock.json'), "`n", [Text.UTF8Encoding]::new($false))
}

try {
    $root = New-DependencyFixture
    $first = Ensure-FrontendDependencies -ProjectRoot $root -Toolchain $testToolchain -Installer ${function:Install-DependencyFixture}
    Assert-DependencyTest $first.Installed 'missing node_modules causes installation'
    Assert-DependencyTest (Test-Path -LiteralPath $first.StampPath) 'successful installation stores a stamp'
    $before = $script:installs
    $again = Ensure-FrontendDependencies -ProjectRoot $root -Toolchain $testToolchain -Installer ${function:Install-DependencyFixture}
    Assert-DependencyTest (-not $again.Installed -and $script:installs -eq $before) 'unchanged inputs reuse dependencies'
    Change-DependencyLockBytes $root
    $changed = Ensure-FrontendDependencies -ProjectRoot $root -Toolchain $testToolchain -Installer ${function:Install-DependencyFixture}
    Assert-DependencyTest ($changed.Installed -and $changed.Fingerprint -ne $first.Fingerprint) 'lockfile byte changes invalidate the cache'
    $manifestPath = Join-Path $root 'package.json'
    [IO.File]::AppendAllText($manifestPath, "`n", [Text.UTF8Encoding]::new($false))
    $manifestChanged = Ensure-FrontendDependencies -ProjectRoot $root -Toolchain $testToolchain -Installer ${function:Install-DependencyFixture}
    Assert-DependencyTest $manifestChanged.Installed 'manifest changes invalidate the cache even if dependencies stay the same'
    foreach ($entry in @(@('nodeMajor', '26'), @('npm', '12.0.0'), @('platform', 'linux'), @('architecture', 'arm64'))) {
        $toolchain = @{} + $testToolchain
        $toolchain[$entry[0]] = $entry[1]
        $result = Ensure-FrontendDependencies -ProjectRoot $root -Toolchain $toolchain -Installer ${function:Install-DependencyFixture}
        Assert-DependencyTest $result.Installed "toolchain $($entry[0]) changes invalidate the cache"
    }

    $root = New-DependencyFixture
    $before = $script:installs
    $manifest = Get-Content -LiteralPath (Join-Path $root 'package.json') -Raw | ConvertFrom-Json
    $manifest.dependencies.alpha = '2.0.0'
    Write-DependencyFixtureFile (Join-Path $root 'package.json') ($manifest | ConvertTo-Json -Depth 10)
    Assert-DependencyThrows { Ensure-FrontendDependencies -ProjectRoot $root -Toolchain $testToolchain -Installer ${function:Install-DependencyFixture} } 'manifest/lock mismatch fails before installation'
    Assert-DependencyTest ($script:installs -eq $before) 'inconsistent manifests never invoke npm ci'
    Assert-DependencyTest (-not (Test-Path -LiteralPath (Join-Path $root 'node_modules'))) 'inconsistent manifests do not create a dependency tree'

    $root = New-DependencyFixture
    $valid = Ensure-FrontendDependencies -ProjectRoot $root -Toolchain $testToolchain -Installer ${function:Install-DependencyFixture}
    Change-DependencyLockBytes $root
    Assert-DependencyThrows {
        Ensure-FrontendDependencies -ProjectRoot $root -Toolchain $testToolchain -Installer { param($Root) Install-DependencyFixture $Root; throw 'simulated npm ci failure' }
    } 'installation errors are surfaced'
    Assert-DependencyTest (-not (Test-Path -LiteralPath $valid.StampPath)) 'failed reinstall cannot leave a success stamp'
    $before = $script:installs
    Ensure-FrontendDependencies -ProjectRoot $root -Toolchain $testToolchain -Installer ${function:Install-DependencyFixture} | Out-Null
    Assert-DependencyTest ($script:installs -eq $before + 1) 'next attempt retries after a partial failure'

    $root = New-DependencyFixture
    Assert-DependencyThrows {
        Ensure-FrontendDependencies -ProjectRoot $root -Toolchain $testToolchain -Installer { param($Root) New-Item -ItemType Directory -Path (Join-Path $Root 'node_modules') | Out-Null }
    } 'incomplete dependency trees fail verification'
    Assert-DependencyTest (-not (Test-Path -LiteralPath (Join-Path $root 'node_modules/.fishcreen-deps.json'))) 'incomplete trees are not stamped'

    $root = New-DependencyFixture
    Assert-DependencyThrows {
        Ensure-FrontendDependencies -ProjectRoot $root -Toolchain $testToolchain -Installer { param($Root) Install-DependencyFixture $Root; Change-DependencyLockBytes $Root }
    } 'inputs changing during installation fail verification'
    Assert-DependencyTest (-not (Test-Path -LiteralPath (Join-Path $root 'node_modules/.fishcreen-deps.json'))) 'changing inputs cannot receive a success stamp'

    $root = New-DependencyFixture
    Write-DependencyFixtureFile (Join-Path $root 'node_modules') 'do not overwrite this file'
    $before = $script:installs
    Assert-DependencyThrows { Ensure-FrontendDependencies -ProjectRoot $root -Toolchain $testToolchain -Installer ${function:Install-DependencyFixture} } 'a node_modules file is rejected'
    Assert-DependencyTest ($script:installs -eq $before) 'invalid dependency target is never passed to the installer'
    Assert-DependencyTest (([IO.File]::ReadAllText((Join-Path $root 'node_modules'))) -eq 'do not overwrite this file') 'invalid target contents are preserved'

    $root = New-DependencyFixture
    $valid = Ensure-FrontendDependencies -ProjectRoot $root -Toolchain $testToolchain -Installer ${function:Install-DependencyFixture}
    Write-DependencyFixtureFile $valid.StampPath 'invalid json'
    $result = Ensure-FrontendDependencies -ProjectRoot $root -Toolchain $testToolchain -Installer ${function:Install-DependencyFixture}
    Assert-DependencyTest $result.Installed 'corrupt stamps cause a reinstall rather than a crash'
    Remove-Item -LiteralPath (Join-Path $root 'node_modules/alpha/package.json')
    $result = Ensure-FrontendDependencies -ProjectRoot $root -Toolchain $testToolchain -Installer ${function:Install-DependencyFixture}
    Assert-DependencyTest $result.Installed 'a missing declared package invalidates a matching stamp'

    if ($env:OS -eq 'Windows_NT') {
        $root = New-DependencyFixture
        $outside = Join-Path $suiteRoot 'unrelated-packages'
        New-Item -ItemType Directory -Path $outside | Out-Null
        Write-DependencyFixtureFile (Join-Path $outside 'keep.txt') 'unrelated content'
        $junction = Join-Path $root 'node_modules'
        New-Item -ItemType Junction -Path $junction -Value $outside | Out-Null
        try {
            $before = $script:installs
            Assert-DependencyThrows { Ensure-FrontendDependencies -ProjectRoot $root -Toolchain $testToolchain -Installer ${function:Install-DependencyFixture} } 'a node_modules junction is rejected'
            Assert-DependencyTest ($script:installs -eq $before) 'reparse targets never reach the installer'
            Assert-DependencyTest (([IO.File]::ReadAllText((Join-Path $outside 'keep.txt'))) -eq 'unrelated content') 'junction target contents are preserved'
        } finally {
            # Remove only the junction itself before the suite's recursive cleanup.
            $junctionItem = Get-Item -LiteralPath $junction -Force
            if (-not ($junctionItem.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
                throw 'The junction fixture unexpectedly became a regular directory.'
            }
            # Windows PowerShell 5.1 Remove-Item has a junction cleanup bug.
            # Directory.Delete without recursion removes the link, not its target.
            [IO.Directory]::Delete($junction)
        }
    }

    Write-Host "frontend-deps: $script:assertions assertions passed across $script:scenarios isolated projects; no real npm ci was run."
} finally {
    # Resolve and check the unique test-only target before recursive cleanup.
    $resolvedSuite = [IO.Path]::GetFullPath($suiteRoot)
    $resolvedTemp = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
    if ($resolvedSuite.StartsWith($resolvedTemp, [StringComparison]::OrdinalIgnoreCase) -and
        (Split-Path -Leaf $resolvedSuite) -match '^fishcreen-deps-test-[0-9a-f]{32}$') {
        Remove-Item -LiteralPath $resolvedSuite -Recurse -Force
    }
}
