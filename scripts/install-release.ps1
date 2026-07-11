[CmdletBinding()]
param(
    [string]$Version = "",
    [string]$Repository = "adamziel/wasmtime-mysql",
    [string]$Destination = (Get-Location).Path
)

$ErrorActionPreference = "Stop"

if ([string]::IsNullOrWhiteSpace($Version)) {
    $Version = if ($env:VERSION) { $env:VERSION } else { "v0.1.9" }
}

$asset = switch ([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()) {
    "X64" { "windows-x86_64"; break }
    "Arm64" { "windows-aarch64"; break }
    default {
        throw "Unsupported Windows architecture: $([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture)"
    }
}

$baseUrl = "https://github.com/$Repository/releases/download/$Version"
$archive = "wasmtime-mysql-$Version-$asset.zip"
$directory = "wasmtime-mysql-$Version-$asset"
$checksumFile = "SHA256SUMS"

New-Item -ItemType Directory -Force -Path $Destination | Out-Null
$archivePath = Join-Path $Destination $archive
$checksumPath = Join-Path $Destination $checksumFile

Invoke-WebRequest -Uri "$baseUrl/$archive" -OutFile $archivePath
Invoke-WebRequest -Uri "$baseUrl/$checksumFile" -OutFile $checksumPath

$checksumLine = Get-Content $checksumPath | Where-Object {
    $_ -match ("\s\s" + [regex]::Escape($archive) + "$")
} | Select-Object -First 1
if (-not $checksumLine) {
    throw "Checksum for $archive was not found in $checksumFile"
}

$expected = ($checksumLine -split "\s+")[0].ToLowerInvariant()
$actual = (Get-FileHash -Algorithm SHA256 -Path $archivePath).Hash.ToLowerInvariant()
if ($actual -ne $expected) {
    throw "Checksum mismatch for $archive"
}

Expand-Archive -Path $archivePath -DestinationPath $Destination -Force

Write-Host ""
Write-Host "Downloaded and verified $archive."
Write-Host "Next:"
Write-Host "  cd `"$(Join-Path $Destination $directory)`""
Write-Host "  .\scripts\run-server.ps1 -Port 3307"
