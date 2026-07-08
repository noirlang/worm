param(
    [string]$OutputPath = ""
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$RootDir = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$DistDir = Join-Path $RootDir "dist"
$StageDir = Join-Path $DistDir "windows-stage"
$WixDir = Join-Path $DistDir "wix"
$BinaryPath = Join-Path $RootDir "target\release\amele.exe"
$ProductWxs = Join-Path $RootDir "packaging\windows\amele.wxs"
$LicenseRtf = Join-Path $RootDir "packaging\windows\license.rtf"
$IconPath = Join-Path $RootDir "packaging\windows\amele.ico"

if ([string]::IsNullOrWhiteSpace($OutputPath)) {
    $OutputPath = Join-Path $DistDir "amele-windows-x64.msi"
}

function Invoke-CheckedCommand {
    param(
        [Parameter(Mandatory = $true)][string]$Command,
        [Parameter(ValueFromRemainingArguments = $true)][string[]]$Arguments
    )

    & $Command @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "Command failed with exit code $LASTEXITCODE`: $Command $($Arguments -join ' ')"
    }
}

function Find-WixTool {
    param([Parameter(Mandatory = $true)][string]$Name)

    $Command = Get-Command "$Name.exe" -ErrorAction SilentlyContinue
    if ($null -ne $Command) {
        return $Command.Source
    }

    $Candidates = @(
        (Join-Path ${env:ProgramFiles(x86)} "WiX Toolset v3.14\bin\$Name.exe"),
        (Join-Path ${env:ProgramFiles(x86)} "WiX Toolset v3.11\bin\$Name.exe"),
        (Join-Path $env:RUNNER_TEMP "wix314\$Name.exe")
    )
    foreach ($Candidate in $Candidates) {
        if (Test-Path $Candidate) {
            return $Candidate
        }
    }

    throw "$Name.exe not found. Install WiX Toolset 3.14 or add its bin directory to PATH."
}

$CargoContent = Get-Content (Join-Path $RootDir "Cargo.toml") -Raw
$VersionMatch = [regex]::Match($CargoContent, '(?m)^version\s*=\s*"([0-9]+\.[0-9]+\.[0-9]+)"')
if (-not $VersionMatch.Success) {
    throw "Cargo.toml package version could not be detected."
}
$Version = $VersionMatch.Groups[1].Value

if (-not (Test-Path $BinaryPath)) {
    Push-Location $RootDir
    try {
        Invoke-CheckedCommand cargo build --release --locked
    }
    finally {
        Pop-Location
    }
}

if (-not (Test-Path $IconPath)) {
    throw "Windows installer icon not found: $IconPath"
}

Remove-Item $StageDir, $WixDir -Recurse -Force -ErrorAction SilentlyContinue
New-Item (Join-Path $StageDir "bin") -ItemType Directory -Force | Out-Null
New-Item (Join-Path $StageDir "share\amele") -ItemType Directory -Force | Out-Null
New-Item $WixDir -ItemType Directory -Force | Out-Null

Copy-Item $BinaryPath (Join-Path $StageDir "bin\amele.exe")
Copy-Item (Join-Path $RootDir "ui") (Join-Path $StageDir "share\amele\ui") -Recurse
Copy-Item (Join-Path $RootDir "tools") (Join-Path $StageDir "share\amele\tools") -Recurse
New-Item (Join-Path $StageDir "share\amele\vendor") -ItemType Directory -Force | Out-Null
Copy-Item (Join-Path $RootDir "vendor\volatility3") (Join-Path $StageDir "share\amele\vendor\volatility3") -Recurse

$Heat = Find-WixTool "heat"
$Candle = Find-WixTool "candle"
$Light = Find-WixTool "light"
$HarvestedWxs = Join-Path $WixDir "harvested.wxs"
$ProductObject = Join-Path $WixDir "product.wixobj"
$HarvestedObject = Join-Path $WixDir "harvested.wixobj"

Invoke-CheckedCommand -Command $Heat -Arguments @(
    "dir", $StageDir, "-nologo", "-cg", "AmeleFiles", "-dr", "INSTALLFOLDER",
    "-srd", "-sfrag", "-sreg", "-scom", "-ag", "-var", "var.StageDir",
    "-out", $HarvestedWxs
)
Invoke-CheckedCommand -Command $Candle -Arguments @(
    "-nologo", "-arch", "x64", "-dProductVersion=$Version",
    "-dAmeleIcon=$IconPath", "-dLicenseRtf=$LicenseRtf",
    "-out", $ProductObject, $ProductWxs
)
Invoke-CheckedCommand -Command $Candle -Arguments @(
    "-nologo", "-arch", "x64", "-dStageDir=$StageDir",
    "-out", $HarvestedObject, $HarvestedWxs
)
Invoke-CheckedCommand -Command $Light -Arguments @(
    "-nologo", "-ext", "WixUIExtension", "-cultures:en-us",
    "-sice:ICE61", "-sice:ICE60",
    "-out", $OutputPath, $ProductObject, $HarvestedObject
)

Write-Host "Windows MSI written to $OutputPath"
