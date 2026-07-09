# wasmtime-mysql

Experimental harness for running a patched MySQL 8.4 server WebAssembly module
inside a native Wasmtime host executable.

This is not an official MySQL port. It is a working research build path that can
initialize a datadir, listen on TCP, accept concurrent clients, create InnoDB
tables, and insert/query rows.

## Requirements

For the release binaries:

- Linux x86_64, macOS Intel, or macOS Apple Silicon
- Python 3, only if you want to run the included benchmark client
- Optional: a local `mysql` CLI for manual connections

For building from source:

- Rust toolchain
- Docker, for the WASI SDK build scripts

## Quick start from a release

Download the latest release asset for your platform:

```sh
VERSION=v0.1.0
case "$(uname -s)-$(uname -m)" in
  Linux-x86_64)  ASSET=linux-x86_64 ;;
  Darwin-x86_64) ASSET=macos-x86_64 ;;
  Darwin-arm64)  ASSET=macos-aarch64 ;;
  *) echo "unsupported platform: $(uname -s)-$(uname -m)" >&2; exit 1 ;;
esac

curl -L -o "waasmtime-mysql-$VERSION-$ASSET.tar.gz" \
  "https://github.com/adamziel/wasmtime-mysql/releases/download/$VERSION/waasmtime-mysql-$VERSION-$ASSET.tar.gz"
tar -xzf "waasmtime-mysql-$VERSION-$ASSET.tar.gz"
cd "waasmtime-mysql-$VERSION-$ASSET"
```

On macOS, the unsigned binary may need quarantine removed:

```sh
xattr -d com.apple.quarantine ./waasmtime-mysql 2>/dev/null || true
```

Create and initialize a datadir:

```sh
RUN_DIR="$PWD/run"
mkdir -p "$RUN_DIR/tmp"

./waasmtime-mysql \
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
./waasmtime-mysql \
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
target/release/waasmtime-mysql
```

## Initialize a datadir

Create a host directory that will be preopened as guest `/tmp`:

```sh
RUN_DIR="$PWD/build/run"
mkdir -p "$RUN_DIR/tmp"
```

Initialize MySQL with an empty local root password:

```sh
target/release/waasmtime-mysql \
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
target/release/waasmtime-mysql \
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
python3 scripts/bench-tcp.py --clients 16 --rows 2000 --batch-size 100
python3 scripts/bench-tcp.py --clients 32 --rows 1000 --batch-size 100
```

Recent numbers from this workspace, after the TCP/socket and thread-exit fixes:

| Clients | Rows/client | Inserted rows | Counted rows | Elapsed | Rows/sec |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 16 | 2,000 | 32,000 | 32,000 | 1.099 s | 29,113 |
| 32 | 1,000 | 32,000 | 32,000 | 1.048 s | 30,521 |

The 32-client run was also validated afterward with `32` tables and `32,000`
rows visible in the live MySQL schema.

## Limitations

- Experimental only. This is a patched research build, not a supported MySQL or
  Wasmtime product.
- Binary logging is not usable yet in this path; run with `--skip-log-bin`.
- TLS and RSA key generation are disabled in the documented commands.
- Dynamic plugin loading is not implemented in the WASI environment, so some
  component/plugin loads are skipped or fail harmlessly at startup.
- The error message file is not packaged into the guest filesystem yet, so
  startup logs include a missing `errmsg.sys` warning.
- MySQL warns that this WASM target was built without its usual memory barrier
  capability. Treat high-concurrency correctness as something that still needs
  deeper validation.
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
