#requires -Version 5.1
<#
.SYNOPSIS
    Builds Spud Launcher, publishes the bundles to a GitHub Release and pushes
    the tag. One command, so "release" and "update GitHub" never drift apart.

.DESCRIPTION
    Steps, in order:
      1. optionally bump the version (Cargo.toml workspace, tauri.conf.json,
         ui/package.json) and commit it
      2. npm --prefix ui run build
      3. tauri build (MSI + NSIS + portable exe)
      4. copy the three artifacts into release/<version> under hyphenated
         names, regenerate SHA256SUMS.txt
      5. tag v<version>, push main + tag
      6. create (or update) the GitHub Release and upload the assets

    The token is read from the same place git already reads it - the Windows
    Credential Manager entry for github.com - so nothing is stored in the repo.

.PARAMETER Version
    New version to publish, e.g. 0.2.0. Omit to rebuild the current one.

.PARAMETER SkipBuild
    Reuse whatever is already in target/release instead of rebuilding.

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File scripts\publish.ps1 -Version 0.2.0

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File scripts\publish.ps1 -SkipBuild
#>
param(
    [string]$Version,
    [switch]$SkipBuild
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$owner = 'LisiastyCWKS'
$repo = 'Spud-Launcher'
$full = "$owner/$repo"

function Invoke-Native {
    param([string]$File, [string[]]$Arguments)
    $previous = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    # git writes progress to stderr; PowerShell turns each line into an
    # ErrorRecord, which formats as a RemoteException blob. Flatten to text.
    $output = & $File @Arguments 2>&1 | ForEach-Object {
        if ($_ -is [System.Management.Automation.ErrorRecord]) { $_.Exception.Message } else { "$_" }
    }
    $code = $LASTEXITCODE
    $ErrorActionPreference = $previous
    $output | ForEach-Object { $_ }
    if ($code -ne 0) { throw "$File $($Arguments -join ' ') failed with exit code $code" }
}

function Get-GitHubToken {
    $lines = ("protocol=https`nhost=github.com`n`n" | git credential fill) 2>$null
    foreach ($line in $lines) {
        if ($line -match '^password=(.+)$') { return $Matches[1] }
    }
    throw 'No github.com credential in the Windows Credential Manager. Run `git push` once interactively, or `gh auth login`.'
}

function Set-JsonVersion {
    param([string]$Path, [string]$NewVersion)
    $text = [System.IO.File]::ReadAllText($Path)
    $updated = [regex]::Replace($text, '("version"\s*:\s*")[^"]+(")', "`${1}$NewVersion`${2}", 1)
    if ($updated -eq $text) { throw "no version field found in $Path" }
    [System.IO.File]::WriteAllText($Path, $updated)
}

function Set-CargoVersion {
    param([string]$Path, [string]$NewVersion)
    $text = [System.IO.File]::ReadAllText($Path)
    $updated = [regex]::Replace($text, '(?m)^(version\s*=\s*")[^"]+(")', "`${1}$NewVersion`${2}", 1)
    if ($updated -eq $text) { throw "no version field found in $Path" }
    [System.IO.File]::WriteAllText($Path, $updated)
}

# ---------------------------------------------------------------- version ---

$cargoToml = Join-Path $root 'Cargo.toml'
$current = ([regex]::Match((Get-Content $cargoToml -Raw), '(?m)^version\s*=\s*"([^"]+)"')).Groups[1].Value
if (-not $current) { throw 'could not read the workspace version from Cargo.toml' }

if ($Version) {
    if ($Version -notmatch '^\d+\.\d+\.\d+$') { throw "Version must look like 1.2.3, got '$Version'" }
    if ($Version -ne $current) {
        Write-Host "version: $current -> $Version"
        Set-CargoVersion -Path $cargoToml -NewVersion $Version
        Set-JsonVersion -Path (Join-Path $root 'src-tauri\tauri.conf.json') -NewVersion $Version
        Set-JsonVersion -Path (Join-Path $root 'ui\package.json') -NewVersion $Version
        Invoke-Native 'git' @('-C', $root, 'add', 'Cargo.toml', 'ui/package.json', 'src-tauri/tauri.conf.json')
        Invoke-Native 'git' @('-C', $root, 'commit', '-m', "Release $Version")
    }
    $current = $Version
} else {
    Write-Host "version: $current (unchanged)"
}
$tag = "v$current"

# ------------------------------------------------------------------ build ---

if (-not $SkipBuild) {
    # tauri's beforeBuildCommand (`npm run build`) and the CLI itself resolve
    # paths against the current directory, so every build step runs from the
    # repository root regardless of where the script was invoked from.
    Push-Location $root
    try {
        Write-Host 'building frontend'
        $npm = (Get-Command npm.cmd -ErrorAction SilentlyContinue)
        if (-not $npm) { throw 'npm.cmd not found on PATH' }
        Invoke-Native $npm.Source @('--prefix', (Join-Path $root 'ui'), 'run', 'build')

        Write-Host 'building bundles'
        $env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
        Invoke-Native 'npm.cmd' @('exec', '--yes', '--package=@tauri-apps/cli@^2', '--', 'tauri', 'build')
    } finally {
        Pop-Location
    }
} else {
    Write-Host 'build skipped (-SkipBuild)'
}

# --------------------------------------------------------------- artifacts ---

$outDir = Join-Path $root "release\$current"
New-Item -ItemType Directory -Force -Path $outDir | Out-Null

$artifacts = @(
    @{ Src = "target\release\bundle\nsis\Spud Launcher_$current`_x64-setup.exe"; Name = "Spud-Launcher_$current`_x64-setup.exe" },
    @{ Src = "target\release\bundle\msi\Spud Launcher_$current`_x64_en-US.msi";  Name = "Spud-Launcher_$current`_x64_en-US.msi" },
    @{ Src = 'target\release\spud-launcher.exe';                                 Name = "Spud-Launcher_$current`_x64-portable.exe" }
)

# GitHub rewrites spaces in an asset name to dots; publishing hyphenated names
# keeps the download and the checksum file telling the same story.
foreach ($artifact in $artifacts) {
    $source = Join-Path $root $artifact.Src
    if (-not (Test-Path $source)) {
        throw "missing build output: $($artifact.Src) - build it first, or drop -SkipBuild"
    }
    Copy-Item $source (Join-Path $outDir $artifact.Name) -Force
    Write-Host "staged $($artifact.Name)"
}

$sumsPath = Join-Path $outDir 'SHA256SUMS.txt'
$sums = foreach ($artifact in $artifacts) {
    $hash = (Get-FileHash (Join-Path $outDir $artifact.Name) -Algorithm SHA256).Hash.ToLower()
    "$hash  $($artifact.Name)"
}
$sums | Set-Content -Path $sumsPath -Encoding ascii
Write-Host ''
$sums | ForEach-Object { Write-Host "  $_" }
Write-Host ''

# ------------------------------------------------------------------- push ---

$token = Get-GitHubToken
$headers = @{
    Authorization = "token $token"
    'User-Agent'  = 'spud-launcher-release'
    Accept        = 'application/vnd.github+json'
}

Invoke-Native 'git' @('-C', $root, 'push', 'origin', 'main')

$existingTag = & git -C $root tag --list $tag
if (-not $existingTag) {
    Invoke-Native 'git' @('-C', $root, 'tag', '-a', $tag, '-m', "Spud Launcher $current")
    Invoke-Native 'git' @('-C', $root, 'push', 'origin', "refs/tags/$tag")
    Write-Host "tagged $tag"
} else {
    $remoteTag = & git -C $root ls-remote --tags origin "refs/tags/$tag"
    if (-not $remoteTag) {
        Invoke-Native 'git' @('-C', $root, 'push', 'origin', "refs/tags/$tag")
    }
    Write-Host "tag $tag already exists"
}

# ------------------------------------------------------------------ release ---

$previousTag = & git -C $root tag --list 'v*' | Where-Object { $_ -ne $tag } | Sort-Object | Select-Object -Last 1
# A range needs a commit that exists; a fresh repo with two commits has no
# HEAD~20, and git then fails the whole release over a changelog nobody needs.
$changes = if ($previousTag) {
    & git -C $root log --pretty=format:'- %s' "$previousTag..HEAD" 2>$null
} else {
    & git -C $root log --pretty=format:'- %s' -20 2>$null
}
if (-not $changes) { $changes = '- pierwsze wydanie' }
# An array interpolated into a here-string joins with a space; the changelog
# needs one bullet per line.
$changeList = ($changes | Where-Object { $_.Trim() }) -join "`n"

$notes = @"
Spud Launcher $current

## Zmiany

$changeList

## Pliki

| Plik | Do czego |
|---|---|
| ``Spud-Launcher_${current}_x64-setup.exe`` | instalator per-user (NSIS) |
| ``Spud-Launcher_${current}_x64_en-US.msi`` | instalacja przez GPO / ``msiexec`` |
| ``Spud-Launcher_${current}_x64-portable.exe`` | wersja portable, bez instalacji |

Sumy SHA-256 w ``SHA256SUMS.txt``. Wymaga Windows 10/11 x64.
"@

$release = $null
try {
    $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$full/releases/tags/$tag" -Headers $headers -TimeoutSec 30
    Write-Host "release $tag exists, updating notes"
    $body = [System.Text.Encoding]::UTF8.GetBytes((@{ body = $notes } | ConvertTo-Json))
    $release = Invoke-RestMethod -Method Patch -Uri "https://api.github.com/repos/$full/releases/$($release.id)" -Headers $headers -ContentType 'application/json' -Body $body -TimeoutSec 60
} catch {
    if ($_.Exception.Response.StatusCode.value__ -ne 404) { throw }
    Write-Host "creating release $tag"
    $payload = @{
        tag_name         = $tag
        target_commitish = 'main'
        name             = "Spud Launcher $current"
        body             = $notes
        draft            = $false
        prerelease       = $false
    } | ConvertTo-Json
    $body = [System.Text.Encoding]::UTF8.GetBytes($payload)
    $release = Invoke-RestMethod -Method Post -Uri "https://api.github.com/repos/$full/releases" -Headers $headers -ContentType 'application/json' -Body $body -TimeoutSec 60
}

# Replace any asset of the same name so a re-run is idempotent and never leaves
# a stale binary behind a fresh checksum file.
$existing = @{}
foreach ($asset in $release.assets) { $existing[$asset.name] = $asset.id }

foreach ($name in $existing.Keys) {
    Invoke-WebRequest -Method Delete -Uri "https://api.github.com/repos/$full/releases/assets/$($existing[$name])" -Headers $headers -UseBasicParsing -TimeoutSec 60 | Out-Null
    Write-Host "removed old asset $name"
}

$uploads = @()
foreach ($artifact in $artifacts) {
    $uploads += [pscustomobject]@{ Path = (Join-Path $outDir $artifact.Name); Name = $artifact.Name; Type = 'application/octet-stream' }
}
$uploads += [pscustomobject]@{ Path = $sumsPath; Name = 'SHA256SUMS.txt'; Type = 'text/plain' }

foreach ($item in $uploads) {
    $url = "https://uploads.github.com/repos/$full/releases/$($release.id)/assets?name=" + [uri]::EscapeDataString($item.Name)
    $result = Invoke-RestMethod -Method Post -Uri $url -Headers $headers -ContentType $item.Type -InFile $item.Path -TimeoutSec 900
    Write-Host ("uploaded {0} ({1:N2} MiB)" -f $result.name, ($result.size / 1MB))
}

$final = Invoke-RestMethod -Uri "https://api.github.com/repos/$full/releases/tags/$tag" -Headers $headers -TimeoutSec 30
Write-Host ''
Write-Host "RELEASE: $($final.html_url)"
foreach ($asset in $final.assets) { Write-Host ("  {0}  {1:N2} MiB" -f $asset.name, ($asset.size / 1MB)) }
