# Shared by build.ps1 and the local check entry point. No dependencies are
# removed here; npm ci owns reinstalling only this project's node_modules.

function Assert-FrontendDependencyDirectory {
    param([Parameter(Mandatory = $true)][string]$Path, [switch]$MayBeMissing)
    try { $item = Get-Item -LiteralPath $Path -Force -ErrorAction Stop }
    catch [System.Management.Automation.ItemNotFoundException] {
        if ($MayBeMissing) { return }
        throw "Missing dependency directory: $Path"
    }
    if (-not $item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
        throw "Refusing to install dependencies through a file or reparse point: $Path"
    }
}

function Get-FrontendDependencyMetadata {
    param([Parameter(Mandatory = $true)][string]$ProjectRoot)
    # Node is already a build prerequisite. Parsing here also works in Windows
    # PowerShell 5.1, whose ConvertFrom-Json rejects the lockfile's empty key.
    $metadataScript = @'
const fs = require('node:fs');
const path = require('node:path');
const root = process.argv[1];
try {
  const manifest = JSON.parse(fs.readFileSync(path.join(root, 'package.json'), 'utf8').replace(/^\uFEFF/, ''));
  const lock = JSON.parse(fs.readFileSync(path.join(root, 'package-lock.json'), 'utf8').replace(/^\uFEFF/, ''));
  const lockedRoot = lock.packages && lock.packages[''];
  if (!lockedRoot || lock.lockfileVersion < 2) throw new Error('A lockfile with root package metadata is required');
  if (manifest.name !== lockedRoot.name || manifest.version !== lockedRoot.version) throw new Error('package.json and package-lock.json root metadata differ; refresh the lockfile first');
  for (const key of ['dependencies', 'devDependencies', 'optionalDependencies', 'peerDependencies', 'peerDependenciesMeta']) {
    const normalize = value => JSON.stringify(Object.entries(value || {}).sort(([a], [b]) => a.localeCompare(b)));
    if (normalize(manifest[key]) !== normalize(lockedRoot[key])) throw new Error('package.json and package-lock.json differ in ' + key + '; refresh the lockfile first');
  }
  const packages = Object.keys({ ...manifest.dependencies, ...manifest.devDependencies }).sort();
  if (packages.some(name => !/^(?:@[^/\\.][^/\\]*\/)?[^/\\.][^/\\]*$/.test(name))) throw new Error('Invalid dependency package name');
  console.log(JSON.stringify({ nodeMajor: process.versions.node.split('.')[0], platform: process.platform, architecture: process.arch, packages }));
} catch (error) {
  console.error(error.message);
  process.exitCode = 1;
}
'@
    $metadataJson = & node -e $metadataScript $ProjectRoot
    if ($LASTEXITCODE -ne 0) { throw 'Frontend manifest/lockfile validation failed before dependency installation.' }
    return ($metadataJson | ConvertFrom-Json)
}

function Get-FrontendDependencyFingerprint {
    param(
        [Parameter(Mandatory = $true)][string]$ProjectRoot,
        [Parameter(Mandatory = $true)]$Metadata,
        [Parameter(Mandatory = $true)]$Toolchain
    )
    $manifestHash = (Get-FileHash -LiteralPath (Join-Path $ProjectRoot 'package.json') -Algorithm SHA256).Hash
    $lockHash = (Get-FileHash -LiteralPath (Join-Path $ProjectRoot 'package-lock.json') -Algorithm SHA256).Hash
    $fingerprintInput = [ordered]@{
        schema = 1
        manifest = $manifestHash
        lockfile = $lockHash
        nodeMajor = [string]$Toolchain.nodeMajor
        npm = [string]$Toolchain.npm
        platform = [string]$Toolchain.platform
        architecture = [string]$Toolchain.architecture
    } | ConvertTo-Json -Compress
    $sha = [Security.Cryptography.SHA256]::Create()
    try {
        return ([BitConverter]::ToString($sha.ComputeHash([Text.Encoding]::UTF8.GetBytes($fingerprintInput)))).Replace('-', '')
    } finally { $sha.Dispose() }
}

function Test-FrontendDependencyPackages {
    param([Parameter(Mandatory = $true)][string]$ModulesPath, [string[]]$Packages)
    foreach ($package in $Packages) {
        $manifest = Join-Path (Join-Path $ModulesPath $package) 'package.json'
        if (-not (Test-Path -LiteralPath $manifest -PathType Leaf)) { return $false }
    }
    return $true
}

function Ensure-FrontendDependencies {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory = $true)][string]$ProjectRoot,
        # Test seams: normal callers omit both parameters.
        [scriptblock]$Installer,
        $Toolchain
    )
    $dependencyRoot = [IO.Path]::GetFullPath((Resolve-Path -LiteralPath $ProjectRoot -ErrorAction Stop).Path)
    if ($dependencyRoot.TrimEnd('\', '/') -eq [IO.Path]::GetPathRoot($dependencyRoot).TrimEnd('\', '/')) {
        throw 'A filesystem root is not a valid frontend project directory.'
    }
    Assert-FrontendDependencyDirectory -Path $dependencyRoot
    foreach ($name in @('package.json', 'package-lock.json')) {
        if (-not (Test-Path -LiteralPath (Join-Path $dependencyRoot $name) -PathType Leaf)) { throw "Missing frontend file: $name" }
    }
    $modulesPath = Join-Path $dependencyRoot 'node_modules'
    Assert-FrontendDependencyDirectory -Path $modulesPath -MayBeMissing
    $stampPath = Join-Path $modulesPath '.fishcreen-deps.json'
    if (Test-Path -LiteralPath $stampPath) {
        $stampItem = Get-Item -LiteralPath $stampPath -Force
        if ($stampItem.PSIsContainer -or ($stampItem.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
            throw 'Dependency stamp must be a regular file inside this project.'
        }
    }

    $metadata = Get-FrontendDependencyMetadata -ProjectRoot $dependencyRoot
    if ($null -eq $Toolchain) {
        $npmVersion = (& npm.cmd --version | Out-String).Trim()
        if ($LASTEXITCODE -ne 0 -or -not $npmVersion) { throw 'Unable to read npm version.' }
        $Toolchain = [ordered]@{ nodeMajor = $metadata.nodeMajor; npm = $npmVersion; platform = $metadata.platform; architecture = $metadata.architecture }
    }
    foreach ($name in @('nodeMajor', 'npm', 'platform', 'architecture')) {
        if (-not [string]$Toolchain.$name) { throw "Missing toolchain fingerprint field: $name" }
    }
    $fingerprint = Get-FrontendDependencyFingerprint -ProjectRoot $dependencyRoot -Metadata $metadata -Toolchain $Toolchain
    $existingFingerprint = $null
    if (Test-Path -LiteralPath $stampPath -PathType Leaf) {
        try { $existingFingerprint = (Get-Content -LiteralPath $stampPath -Raw | ConvertFrom-Json).fingerprint }
        catch { Write-Verbose 'The dependency stamp is invalid; reinstalling dependencies.' }
    }
    if ($existingFingerprint -eq $fingerprint -and (Test-FrontendDependencyPackages -ModulesPath $modulesPath -Packages @($metadata.packages))) {
        return [pscustomobject]@{ Installed = $false; Fingerprint = $fingerprint; StampPath = $stampPath }
    }

    # An unsuccessful or partially completed npm ci must never leave behind a
    # success marker that could cause a later build to reuse that partial tree.
    if (Test-Path -LiteralPath $stampPath) { Remove-Item -LiteralPath $stampPath -Force }
    if ($null -eq $Installer) {
        $Installer = {
            param($Root)
            Push-Location -LiteralPath $Root
            try {
                & npm.cmd ci --no-audit --no-fund | Out-Host
                if ($LASTEXITCODE -ne 0) { throw 'Frontend dependency installation failed.' }
            } finally { Pop-Location }
        }
    }
    & $Installer $dependencyRoot | Out-Host
    Assert-FrontendDependencyDirectory -Path $modulesPath
    if (-not (Test-FrontendDependencyPackages -ModulesPath $modulesPath -Packages @($metadata.packages))) {
        throw 'Dependency installation did not produce the declared packages; no success stamp was saved.'
    }
    $after = Get-FrontendDependencyMetadata -ProjectRoot $dependencyRoot
    if ((Get-FrontendDependencyFingerprint -ProjectRoot $dependencyRoot -Metadata $after -Toolchain $Toolchain) -ne $fingerprint) {
        throw 'Frontend inputs changed during installation; no success stamp was saved. Retry the build.'
    }

    $temporaryStamp = "$stampPath.$([Guid]::NewGuid().ToString('N')).tmp"
    try {
        $stamp = [ordered]@{ schema = 1; fingerprint = $fingerprint; installedAtUtc = [DateTime]::UtcNow.ToString('o') } | ConvertTo-Json -Compress
        [IO.File]::WriteAllText($temporaryStamp, $stamp, [Text.UTF8Encoding]::new($false))
        # Do not overwrite a marker concurrently created by a different build.
        Move-Item -LiteralPath $temporaryStamp -Destination $stampPath -ErrorAction Stop
    } finally {
        if (Test-Path -LiteralPath $temporaryStamp) { Remove-Item -LiteralPath $temporaryStamp -Force }
    }
    return [pscustomobject]@{ Installed = $true; Fingerprint = $fingerprint; StampPath = $stampPath }
}
