[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$Runner,
    [int]$Port = 33307
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$runDirectory = Join-Path $env:RUNNER_TEMP "wasmtime-mysql-windows-smoke-$Port"
$serverLog = Join-Path $runDirectory "server.out"
$serverError = Join-Path $runDirectory "server.err"

Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $runDirectory
New-Item -ItemType Directory -Force -Path (Join-Path $runDirectory "tmp") | Out-Null

$commonArgs = @(
    "--no-default-preopen",
    "--preopen", "$runDirectory=/tmp",
    "--env", "TMPDIR=/tmp/tmp",
    "--env", "HOME=/tmp",
    "--"
)
$initializeArgs = $commonArgs + @(
    "--no-defaults",
    "--initialize-insecure",
    "--skip-networking",
    "--console",
    "--datadir=/tmp/data",
    "--tmpdir=/tmp/tmp",
    "--log-error=/tmp/mysqld-init.err",
    "--auto-generate-certs=OFF",
    "--sha256-password-auto-generate-rsa-keys=OFF",
    "--caching-sha2-password-auto-generate-rsa-keys=OFF"
)

& $Runner @initializeArgs
if ($LASTEXITCODE -ne 0) {
    throw "MySQL initialization failed with exit code $LASTEXITCODE"
}

$serverArgs = $commonArgs + @(
    "--no-defaults",
    "--console",
    "--datadir=/tmp/data",
    "--tmpdir=/tmp/tmp",
    "--log-error=/tmp/mysqld-runtime.err",
    "--port=$Port",
    "--bind-address=127.0.0.1",
    "--skip-log-bin",
    "--auto-generate-certs=OFF",
    "--sha256-password-auto-generate-rsa-keys=OFF",
    "--caching-sha2-password-auto-generate-rsa-keys=OFF"
)
$quotedArgs = ($serverArgs | ForEach-Object { '"' + $_.Replace('"', '\"') + '"' }) -join " "
$server = Start-Process -FilePath $Runner -ArgumentList $quotedArgs -PassThru -RedirectStandardOutput $serverLog -RedirectStandardError $serverError

try {
    $connected = $false
    for ($attempt = 0; $attempt -lt 60; $attempt++) {
        if ($server.HasExited) {
            throw "MySQL exited before accepting a connection (exit code $($server.ExitCode))"
        }
        & python "$PSScriptRoot/bench-tcp.py" --port $Port --clients 1 --rows 5 --batch-size 5
        if ($LASTEXITCODE -eq 0) {
            $connected = $true
            break
        }
        Start-Sleep -Milliseconds 500
    }
    if (-not $connected) {
        throw "MySQL did not accept a TCP connection on port $Port"
    }
} finally {
    if (-not $server.HasExited) {
        Stop-Process -Id $server.Id -Force
        $server.WaitForExit()
    }
    if (Test-Path $serverLog) {
        Get-Content $serverLog
    }
    if (Test-Path $serverError) {
        Get-Content $serverError
    }
}
