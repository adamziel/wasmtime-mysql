[CmdletBinding()]
param(
    [int]$Port = 3307,
    [string]$RunDirectory = ""
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$runner = Join-Path $root "wasmtime-mysql.exe"

if (-not (Test-Path -LiteralPath $runner -PathType Leaf)) {
    throw "wasmtime-mysql.exe was not found beside this release's scripts directory"
}

if ([string]::IsNullOrWhiteSpace($RunDirectory)) {
    $RunDirectory = Join-Path $root "run"
}

$RunDirectory = [System.IO.Path]::GetFullPath($RunDirectory)
$dataDirectory = Join-Path $RunDirectory "data"
$tmpDirectory = Join-Path $RunDirectory "tmp"
New-Item -ItemType Directory -Force -Path $tmpDirectory | Out-Null

$commonArgs = @(
    "--no-default-preopen",
    "--preopen", "$RunDirectory=/tmp",
    "--env", "TMPDIR=/tmp/tmp",
    "--env", "HOME=/tmp",
    "--"
)

if (-not (Test-Path -LiteralPath (Join-Path $dataDirectory "mysql") -PathType Container)) {
    Write-Host "Initializing MySQL data directory in $dataDirectory"
    $initializeArgs = $commonArgs + @(
        "--no-defaults",
        "--initialize-insecure",
        "--skip-networking",
        "--console",
        "--datadir=/tmp/data",
        "--tmpdir=/tmp/tmp",
        "--log-error=/tmp/mysqld-init.err",
        "--log-error-verbosity=3",
        "--auto-generate-certs=OFF",
        "--sha256-password-auto-generate-rsa-keys=OFF",
        "--caching-sha2-password-auto-generate-rsa-keys=OFF"
    )
    & $runner @initializeArgs
    if ($LASTEXITCODE -ne 0) {
        exit $LASTEXITCODE
    }
}

Write-Host "Starting MySQL on 127.0.0.1:$Port"
$serverArgs = $commonArgs + @(
    "--no-defaults",
    "--console",
    "--datadir=/tmp/data",
    "--tmpdir=/tmp/tmp",
    "--log-error=/tmp/mysqld-runtime.err",
    "--log-error-verbosity=3",
    "--port=$Port",
    "--bind-address=127.0.0.1",
    "--skip-log-bin",
    "--auto-generate-certs=OFF",
    "--sha256-password-auto-generate-rsa-keys=OFF",
    "--caching-sha2-password-auto-generate-rsa-keys=OFF"
)

& $runner @serverArgs
exit $LASTEXITCODE
