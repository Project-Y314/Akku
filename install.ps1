<# 
.SYNOPSIS
    One-command installer for Akku on Windows.
    Usage: irm https://raw.githubusercontent.com/ybreakless/Akku/main/install.ps1 | iex

.DESCRIPTION
    Downloads the latest Akku release binaries from GitHub and installs them to
    %LOCALAPPDATA%\Akku\bin, adding that folder to the user PATH.

.NOTES
    Requires Windows 10/11, PowerShell 5.1+
#>

#requires -Version 5.1

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$repo = "ybreakless/Akku"
$installDir = Join-Path $env:LOCALAPPDATA "Akku\bin"
$zipPath = Join-Path $env:TEMP "akku-release.zip"

Write-Host "🔋 Akku installer" -ForegroundColor Cyan
Write-Host "Repository: $repo" -ForegroundColor Gray
Write-Host "Install directory: $installDir" -ForegroundColor Gray

# Get latest release info
Write-Host "`n📡 Fetching latest release..." -ForegroundColor Yellow
$release = Invoke-RestMethod -Uri "https://api.github.com/repos/$repo/releases/latest" -Headers @{Accept = "application/vnd.github.v3+json"}
$version = $release.tag_name
Write-Host "   Latest: $version" -ForegroundColor Green

# Find the two .exe assets
$akkuAsset = $release.assets | Where-Object { $_.name -eq 'akku.exe' } | Select-Object -First 1
$cliAsset  = $release.assets | Where-Object { $_.name -eq 'akku-cli.exe' } | Select-Object -First 1

if (-not $akkuAsset -or -not $cliAsset) {
    Write-Error "❌ Could not find akku.exe and/or akku-cli.exe in release assets"
    exit 1
}

# Create install directory
if (-not (Test-Path $installDir)) {
    New-Item -ItemType Directory -Path $installDir -Force | Out-Null
    Write-Host "📁 Created $installDir" -ForegroundColor Green
}

# Download both binaries
Write-Host "`n⬇️  Downloading binaries..." -ForegroundColor Yellow
foreach ($asset in @($akkuAsset, $cliAsset)) {
    $dest = Join-Path $installDir $asset.name
    Write-Host "   $($asset.name) ... " -NoNewline
    Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $dest -UseBasicParsing
    Write-Host "✅" -ForegroundColor Green
}

# Add to user PATH (persistent)
Write-Host "`n🔗 Adding to user PATH..." -ForegroundColor Yellow
$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($userPath -notlike "*$installDir*") {
    $newPath = if ($userPath) { "$userPath;$installDir" } else { $installDir }
    [Environment]::SetEnvironmentVariable("Path", $newPath, "User")
    Write-Host "   Added $installDir to user PATH" -ForegroundColor Green
    Write-Host "   ⚠️  Restart your terminal / re-login for PATH to take effect" -ForegroundColor Yellow
} else {
    Write-Host "   Already in PATH" -ForegroundColor Green
}

# Verify
Write-Host "`n✅ Verification:" -ForegroundColor Cyan
& "$installDir\akku.exe" --help 2>&1 | Select-Object -First 3
& "$installDir\akku-cli.exe" --help 2>&1 | Select-Object -First 3

Write-Host "`n🎉 Akku $version installed!" -ForegroundColor Cyan
Write-Host "Run: akku.exe           (tray app)" -ForegroundColor Gray
Write-Host "Run: akku.exe --demo    (demo mode, no hardware needed)" -ForegroundColor Gray
Write-Host "Run: akku-cli.exe list  (CLI)" -ForegroundColor Gray