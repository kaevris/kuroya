[CmdletBinding()]
param(
    [switch]$SkipBuild,
    [switch]$ValidateOnly,
    [string]$InnoCompilerPath
)

$ErrorActionPreference = 'Stop'

$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$ReleaseExe = Join-Path $RepoRoot 'target\release\kuroya.exe'
$InnoScript = Join-Path $PSScriptRoot 'kuroya.iss'
$SupportedFileTypesManifest = Join-Path $RepoRoot 'crates\kuroya-core\supported-file-types.txt'
$GeneratedInstallerDir = Join-Path $RepoRoot 'target\installer'
$SupportedFileTypesInclude = Join-Path $GeneratedInstallerDir 'supported-file-types.iss'

function Resolve-InnoCompiler {
    param([string]$ExplicitPath)

    $candidatePaths = @()
    if ($ExplicitPath) {
        $candidatePaths += $ExplicitPath
    }
    if ($env:INNO_SETUP_COMPILER) {
        $candidatePaths += $env:INNO_SETUP_COMPILER
    }
    $candidatePaths += Join-Path $RepoRoot '.tools\InnoSetup6\ISCC.exe'
    if (${env:ProgramFiles(x86)}) {
        $candidatePaths += Join-Path ${env:ProgramFiles(x86)} 'Inno Setup 6\ISCC.exe'
    }
    if ($env:ProgramFiles) {
        $candidatePaths += Join-Path $env:ProgramFiles 'Inno Setup 6\ISCC.exe'
    }
    if ($env:LOCALAPPDATA) {
        $candidatePaths += Join-Path $env:LOCALAPPDATA 'Programs\Inno Setup 6\ISCC.exe'
    }

    foreach ($candidate in $candidatePaths) {
        if ($candidate -and (Test-Path -LiteralPath $candidate)) {
            return (Resolve-Path -LiteralPath $candidate).Path
        }
    }

    $command = Get-Command ISCC.exe -ErrorAction SilentlyContinue
    if ($command) {
        return $command.Source
    }

    throw @'
Inno Setup compiler not found.
Install Inno Setup 6, add ISCC.exe to PATH, set INNO_SETUP_COMPILER, or pass -InnoCompilerPath.
'@
}

function Write-SupportedFileTypesInclude {
    param(
        [string]$ManifestPath,
        [string]$OutputPath
    )

    if (-not (Test-Path -LiteralPath $ManifestPath -PathType Leaf)) {
        throw "Supported file type manifest not found: $ManifestPath"
    }

    $extensions = New-Object System.Collections.Generic.List[string]
    $seen = New-Object 'System.Collections.Generic.HashSet[string]' ([System.StringComparer]::OrdinalIgnoreCase)
    foreach ($rawLine in Get-Content -LiteralPath $ManifestPath) {
        $line = $rawLine.Trim()
        if (-not $line -or $line.StartsWith('#')) {
            continue
        }

        $fields = @($line -split '\s+')
        if ($fields.Count -ne 2) {
            throw "Invalid supported file type row: $rawLine"
        }
        $extension = $fields[0]
        $languageId = $fields[1]
        if ($extension -cne $extension.ToLowerInvariant() -or $extension -notmatch '^[a-z0-9][a-z0-9+_-]*$') {
            throw "Invalid supported file extension: $extension"
        }
        if ($languageId -cne $languageId.ToLowerInvariant() -or $languageId -notmatch '^[a-z][a-z0-9_-]*$') {
            throw "Invalid supported file language ID: $languageId"
        }
        if (-not $seen.Add($extension)) {
            throw "Duplicate supported file extension: $extension"
        }
        $extensions.Add($extension)
    }

    if ($extensions.Count -eq 0) {
        throw 'Supported file type manifest is empty'
    }

    $lines = New-Object System.Collections.Generic.List[string]
    $lines.Add('; Generated from crates\kuroya-core\supported-file-types.txt. Do not edit.')
    foreach ($extension in $extensions) {
        $extensionKey = "Software\Classes\.$extension"
        $openWithKey = "$extensionKey\OpenWithProgids"
        $lines.Add("Root: HKA64; Subkey: `"$extensionKey`"; Flags: uninsdeletekeyifempty; Tasks: associatewithfiles")
        $lines.Add("Root: HKA64; Subkey: `"$openWithKey`"; ValueType: string; ValueName: `"{#SourceFileProgId}`"; ValueData: `"`"; Flags: uninsdeletevalue uninsdeletekeyifempty; Tasks: associatewithfiles")
        $lines.Add("Root: HKA64; Subkey: `"$openWithKey`"; ValueType: none; ValueName: `"{#SourceFileProgId}`"; Flags: deletevalue; Tasks: not associatewithfiles")
    }

    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $OutputPath) | Out-Null
    $utf8WithoutBom = New-Object System.Text.UTF8Encoding($false)
    [System.IO.File]::WriteAllLines($OutputPath, $lines, $utf8WithoutBom)
    return $extensions.Count
}

Push-Location $RepoRoot
try {
    if (-not $SkipBuild -and -not $ValidateOnly) {
        cargo build -p kuroya-app --release
        if ($LASTEXITCODE -ne 0) {
            throw "cargo build failed with exit code $LASTEXITCODE"
        }
    }

    if (-not (Test-Path -LiteralPath $ReleaseExe)) {
        throw "Release binary not found: $ReleaseExe"
    }

    $InnoCompiler = Resolve-InnoCompiler -ExplicitPath $InnoCompilerPath

    $manifest = Get-Content -LiteralPath (Join-Path $RepoRoot 'crates\kuroya-app\Cargo.toml')
    $versionLine = $manifest | Where-Object { $_ -match '^version\s*=\s*"([^"]+)"' } | Select-Object -First 1
    if (-not $versionLine) {
        throw 'Could not read kuroya-app version from Cargo.toml'
    }
    $Version = [regex]::Match($versionLine, '^version\s*=\s*"([^"]+)"').Groups[1].Value

    $supportedFileTypeCount = Write-SupportedFileTypesInclude `
        -ManifestPath $SupportedFileTypesManifest `
        -OutputPath $SupportedFileTypesInclude

    if (-not $ValidateOnly) {
        New-Item -ItemType Directory -Force -Path (Join-Path $RepoRoot 'dist') | Out-Null
        Get-ChildItem -LiteralPath (Join-Path $RepoRoot 'dist') -Filter 'Kuroya-Setup-*.exe' -ErrorAction SilentlyContinue |
            Remove-Item -Force
    }

    $compilerArguments = @("/DSourceRoot=$RepoRoot", "/DAppVersion=$Version")
    if ($ValidateOnly) {
        $compilerArguments += '/Qp'
        $compilerArguments += '/O-'
    }
    $compilerArguments += $InnoScript
    & $InnoCompiler @compilerArguments
    if ($LASTEXITCODE -ne 0) {
        throw "Inno Setup failed with exit code $LASTEXITCODE"
    }

    if ($ValidateOnly) {
        [pscustomobject]@{
            Script = $InnoScript
            SupportedFileTypes = $supportedFileTypeCount
            Valid = $true
        }
        return
    }

    $installerPath = Join-Path $RepoRoot "dist\Kuroya-Setup-$Version.exe"
    if (-not (Test-Path -LiteralPath $installerPath)) {
        throw "Installer was not created: $installerPath"
    }

    Get-Item -LiteralPath $installerPath | Select-Object FullName, Length, LastWriteTime
} finally {
    Pop-Location
}
