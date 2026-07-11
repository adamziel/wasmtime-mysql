# wasmtime-mysql

Experimental harness for running a patched MySQL 8.4 server WebAssembly module
inside a native Wasmtime host executable.

This is not an official MySQL port. It is a working research build path that can
initialize a datadir, listen on TCP, accept concurrent clients, create InnoDB
tables, and insert/query rows.

## Requirements

For the release binaries:

- Linux x86_64, Windows x86_64 or ARM64, macOS Intel, or macOS Apple Silicon
- Python 3, only if you want to run the included benchmark client
- Optional: a local `mysql` CLI for manual connections

For building from source:

- Rust toolchain
- Docker, for the WASI SDK build scripts

## Quick start from a release

### Linux and macOS

Download the latest release asset for your platform:

```sh
curl -fsSL https://raw.githubusercontent.com/adamziel/wasmtime-mysql/main/scripts/install-release.sh | sh
cd wasmtime-mysql-v0.1.6-*
```

On macOS, the unsigned binary may need quarantine removed:

```sh
xattr -d com.apple.quarantine ./wasmtime-mysql 2>/dev/null || true
```

### Windows

In PowerShell, the installer detects Intel versus ARM64, verifies the release
checksum, and prints the directory it extracted:

```powershell
Invoke-RestMethod https://raw.githubusercontent.com/adamziel/wasmtime-mysql/main/scripts/install-release.ps1 | Invoke-Expression
cd .\wasmtime-mysql-v0.1.6-windows-aarch64
.\scripts\run-server.ps1 -Port 3307
```

Use `windows-x86_64` instead of `windows-aarch64` on an Intel or AMD machine.
The launcher initializes `run\data` once, then starts MySQL on `127.0.0.1`.

Connect from a second PowerShell window:

```powershell
mysql.exe --protocol=TCP -h127.0.0.1 -P3307 -uroot --ssl-mode=DISABLED
```

The remaining manual commands use POSIX shell syntax and apply to Linux and
macOS.

Create and initialize a datadir:

```sh
RUN_DIR="$PWD/run"
mkdir -p "$RUN_DIR/tmp"

./wasmtime-mysql \
  --no-default-preopen \
  --preopen "$RUN_DIR=/tmp" \
  --env TMPDIR=/tmp/tmp \
  --env HOME=/tmp \
  -- \
  --no-defaults \
  --initialize-insecure \
  --skip-networking \
  --console \
  --datadir=/tmp/data \
  --tmpdir=/tmp/tmp \
  --log-error=/tmp/mysqld-init.err \
  --log-error-verbosity=3 \
  --auto-generate-certs=OFF \
  --sha256-password-auto-generate-rsa-keys=OFF \
  --caching-sha2-password-auto-generate-rsa-keys=OFF
```

Start the server on `127.0.0.1:3307`:

```sh
./wasmtime-mysql \
  --no-default-preopen \
  --preopen "$RUN_DIR=/tmp" \
  --env TMPDIR=/tmp/tmp \
  --env HOME=/tmp \
  -- \
  --no-defaults \
  --console \
  --datadir=/tmp/data \
  --tmpdir=/tmp/tmp \
  --log-error=/tmp/mysqld-runtime.err \
  --log-error-verbosity=3 \
  --port=3307 \
  --bind-address=127.0.0.1 \
  --skip-log-bin \
  --auto-generate-certs=OFF \
  --sha256-password-auto-generate-rsa-keys=OFF \
  --caching-sha2-password-auto-generate-rsa-keys=OFF
```

Connect from another terminal:

```sh
mysql --protocol=TCP -h127.0.0.1 -P3307 -uroot --ssl-mode=DISABLED
```

Or run the included dependency-free benchmark/connectivity client:

```sh
python3 scripts/bench-tcp.py --clients 1 --rows 5 --batch-size 5
```

## Build

Fetch the pinned MySQL source and build the patched WASI module:

```sh
./scripts/fetch-mysql-source.sh
./scripts/probe-mysql-wasi-port.sh
```

Bundle the resulting `mysqld` WebAssembly module into one native executable:

```sh
./scripts/build-single.sh build/mysql-wasi-port/build/runtime_output_directory/mysqld
```

The runner is written to:

```sh
target/release/wasmtime-mysql
```

## Initialize a datadir

Create a host directory that will be preopened as guest `/tmp`:

```sh
RUN_DIR="$PWD/build/run"
mkdir -p "$RUN_DIR/tmp"
```

Initialize MySQL with an empty local root password:

```sh
target/release/wasmtime-mysql \
  --no-default-preopen \
  --preopen "$RUN_DIR=/tmp" \
  --env TMPDIR=/tmp/tmp \
  --env HOME=/tmp \
  -- \
  --no-defaults \
  --initialize-insecure \
  --skip-networking \
  --console \
  --datadir=/tmp/data \
  --tmpdir=/tmp/tmp \
  --log-error=/tmp/mysqld-init.err \
  --log-error-verbosity=3 \
  --auto-generate-certs=OFF \
  --sha256-password-auto-generate-rsa-keys=OFF \
  --caching-sha2-password-auto-generate-rsa-keys=OFF
```

## Run the server

Start MySQL on `127.0.0.1:3307`:

```sh
target/release/wasmtime-mysql \
  --no-default-preopen \
  --preopen "$RUN_DIR=/tmp" \
  --env TMPDIR=/tmp/tmp \
  --env HOME=/tmp \
  -- \
  --no-defaults \
  --console \
  --datadir=/tmp/data \
  --tmpdir=/tmp/tmp \
  --log-error=/tmp/mysqld-runtime.err \
  --log-error-verbosity=3 \
  --port=3307 \
  --bind-address=127.0.0.1 \
  --skip-log-bin \
  --auto-generate-certs=OFF \
  --sha256-password-auto-generate-rsa-keys=OFF \
  --caching-sha2-password-auto-generate-rsa-keys=OFF
```

`--skip-log-bin` is currently important; the binary log path can abort the
server during DDL commits.

## Stop the server

On Linux and macOS, the first `Ctrl+C` or `SIGTERM` is a graceful stop: the
runner stops the listener, lets MySQL drain connections, and lets InnoDB flush
before the process exits. A second `Ctrl+C` exits immediately and is not a
clean shutdown.

On Windows, use SQL `SHUTDOWN` for a clean stop. Console `Ctrl+C` terminates
the process but does not yet enter the Unix-style graceful shutdown path.

An SQL shutdown uses the same path:

```sh
mysql --protocol=TCP -h127.0.0.1 -P3307 -uroot --ssl-mode=DISABLED \
  -e 'SHUTDOWN'
```

Use `SHUTDOWN`, not a MariaDB `mysqladmin shutdown` command. MySQL 8.4 removed
the legacy protocol command used by that `mysqladmin` implementation.

Do not use `SIGKILL` unless the server is already unrecoverable. InnoDB crash
recovery is expected after an abrupt stop.

## Connect

With the MySQL CLI:

```sh
mysql --protocol=TCP -h127.0.0.1 -P3307 -uroot --ssl-mode=DISABLED
```

Then run normal SQL:

```sql
CREATE DATABASE demo;
CREATE TABLE demo.t (id INT PRIMARY KEY, payload VARCHAR(64)) ENGINE=InnoDB;
INSERT INTO demo.t VALUES (1, 'hello from wasmtime');
SELECT * FROM demo.t;
```

The benchmark client also serves as a dependency-free connectivity check:

```sh
python3 scripts/bench-tcp.py --clients 1 --rows 5 --batch-size 5
```

## Benchmark

The included benchmark client opens concurrent TCP connections, creates one
InnoDB table per client in the `bench` schema, inserts rows in batches, and
verifies `COUNT(*)` for each table.

```sh
python3 scripts/bench-tcp.py --clients 1 --rows 20000 --batch-size 100
python3 scripts/bench-tcp.py --clients 4 --rows 5000 --batch-size 100
```

Fresh Linux x86_64 source-build numbers from this workspace are below. They
are useful for regression tracking, not as a hardware-independent claim.

| Workload | Result |
| --- | ---: |
| 1 client, 20,000 inserted rows in 100-row statements | 26,564 rows/sec |
| 4 clients, 20,000 inserted rows in 100-row statements | 34,207 rows/sec |
| 60,000 `SELECT 1` round trips, one connection | 12,811 queries/sec |
| 5,000 single-row `BEGIN` / `INSERT` / `COMMIT` transactions | 524 transactions/sec |
| 4 clients, 4,000 single-row transactions total | 1,275 transactions/sec |

The transaction figures use the default durable InnoDB commit behavior. Bulk
rows/sec is not a proxy for small-query or commit-heavy WordPress work.

## Limitations

- Experimental only. This is a patched research build, not a supported MySQL or
  Wasmtime product.
- Binary logging is not usable yet in this path; run with `--skip-log-bin`.
  The GTID-table compression worker is disabled in this WASI build because its
  join path cannot be represented safely by the current WASI thread ABI.
- TLS and RSA key generation are disabled in the documented commands.
- Dynamic plugin loading is not implemented in the WASI environment, so some
  component/plugin loads are skipped or fail harmlessly at startup.
- The error message file is not packaged into the guest filesystem yet, so
  startup logs include a missing `errmsg.sys` warning.
- The host forwards InnoDB file syncs, parent-directory syncs, and nonblocking
  exclusive data-file locks to the native filesystem. Unix uses POSIX APIs;
  Windows uses `FlushFileBuffers` and `LockFileEx`. Clean shutdown, restart,
  and duplicate-datadir rejection are tested on Unix. The Windows release
  workflow initializes InnoDB and runs TCP DDL/insert smoke coverage, but does
  not yet run the full lifecycle/durability regression there. Power-loss fault
  injection and every DDL crash window are not covered on any platform.
- MySQL warns that this WASM target was built without its usual memory-barrier
  capability. Wasm threads and MySQL mutexes do execute against shared memory,
  so concurrent workload tests exercise real locking. That does not prove that
  every ordering assumption in MySQL's native platform layer is valid here. A
  16-client benchmark has trapped in WASM memory during local testing.
- The build relies on patches under `patches/mysql-wasi/` and generated output
  under `build/`; the generated MySQL source/build trees are intentionally not
  committed.

## Useful development checks

Verify the Rust host without embedding MySQL:

```sh
./scripts/verify-dev-fixture.sh
```

Check formatting and host compilation:

```sh
cargo fmt --check
cargo check --features dev-fixture
```

Run the end-to-end lifecycle regression after building the source port and
bundling the runner:

```sh
./scripts/test-lifecycle.sh
```

It initializes a datadir, checks SQL `SHUTDOWN`, `SIGINT`, and `SIGTERM`,
verifies a committed InnoDB row after restart, rejects a competing process
using the same datadir, and confirms no crash-recovery startup after a clean
stop. Override `MYSQL_CLIENT`, `MYSQL_RUNNER`, or `MYSQL_PORT` when needed.

## Architecture

This is not MySQL compiled into a browser toy. `mysqld` is a patched WASI
module embedded in a native Rust executable using Wasmtime. The host creates
the shared Wasm memory, provides WASIp1 calls, and starts a fresh Wasm instance
for each guest pthread while keeping that memory shared.

The port deliberately routes the parts WASI Preview 1 does not provide through
narrow host imports: TCP sockets, positional file I/O, file sync, directory
sync, and advisory file locks. Paths are restricted to runner preopens; the
documented command maps one host run directory to guest `/tmp`.

The Windows runner is a native MSVC executable, not a Linux compatibility
layer. Its host imports use WinSock, Windows positional file APIs,
`FlushFileBuffers`, and `LockFileEx`, while preserving the guest's POSIX-like
socket and errno ABI at the import boundary.

MySQL's normal Unix signal thread cannot work in this environment. Instead,
the runner owns one shared shutdown flag. On Unix, SQL `SHUTDOWN`, `Ctrl+C`,
and `SIGTERM` set that flag; the listener polls at 100 ms, returns to MySQL's
main thread, and the main thread performs the normal connection and InnoDB
cleanup. On Windows, SQL `SHUTDOWN` uses the same guest flag while console
control handling remains incomplete. That split is intentional: guest-created
signal threads and joins are not a reliable lifecycle mechanism here.
