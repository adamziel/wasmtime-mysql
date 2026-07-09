# Status

## Current Evidence

- The native Wasmtime runner builds and executes an embedded WASIp1 command when using the `dev-fixture` feature.
- The production build path refuses to build unless `MYSQLD_WASM` points to a WebAssembly binary, which prevents accidentally shipping the fixture as MySQL.
- The MySQL source probe fetched upstream tag `mysql-8.4.10` at commit `6adc159923b7b6abbe649949551ec25264c2daf9`.
- The stock Docker/WASI probe reaches MySQL's CMake configure step with wasi-sdk, then fails because MySQL requires 64-bit platforms and `wasm32-wasip1-threads` has 32-bit pointers.
- The static OpenSSL dependency can be built for WASI from OpenSSL `openssl-3.0.21` with `scripts/build-openssl-wasi.sh`.
- The diagnostic MySQL WASI port probe now links `build/mysql-wasi-port/build/runtime_output_directory/mysqld` as a WebAssembly module.
- The linked diagnostic `mysqld` module can be embedded into `target/release/waasmtime-mysql` with `scripts/build-single.sh`.

The stock probe output is still:

```text
CMake Error at CMakeLists.txt:698 (MESSAGE):
  MySQL supports only 64-bit platforms.

-- SIZEOF_VOIDP 4
-- Configuring incomplete, errors occurred!
```

The diagnostic port probe now reaches the final link:

```text
[2131/2131] Linking CXX executable runtime_output_directory/mysqld
```

The resulting artifact observed in this workspace is:

```text
build/mysql-wasi-port/build/runtime_output_directory/mysqld: WebAssembly (wasm) binary module version 0x1 (MVP)
size: 77M
imported shared memory: 128 MiB initial, 1 GiB maximum
```

The bundled executable observed in this workspace is:

```text
target/release/waasmtime-mysql: ELF 64-bit LSB pie executable, x86-64
size: 91M
embedded source: build/mysql-wasi-port/build/runtime_output_directory/mysqld
```

## Diagnostic-Only Port Probe

`scripts/probe-mysql-wasi-port.sh` is intentionally a probe, not a production port. It currently:

- builds static OpenSSL for WASI;
- bypasses MySQL's top-level 64-bit check only when `MYSQL_WASI_PORT_PROBE=ON`;
- disables editline/curses, DNS SRV lookup, MySQL X Plugin, group replication, client authentication plugins, Router, NDB, and selected optional dependencies;
- forces bundled Protobuf to static libraries and uses a native `protoc` only as a build-time generator;
- builds native host tools for generated MySQL sources;
- patches bundled ICU and Abseil code paths that assume POSIX signals or mmap behavior;
- clears false-positive `epoll` and `kqueue` configure detections for WASI;
- injects libc/POSIX no-op shims for APIs such as selected pthread, process, user, mmap, and resource calls;
- stubs unsupported resource-group platform APIs for WASI;
- replaces target-side socket calls with a diagnostic host-import shim;
- links with WebAssembly exceptions, WASI emulation libraries, libunwind, and explicit imported shared-memory limits.

These changes are enough to produce a `mysqld` wasm module, but they are not enough to make Oracle MySQL Server semantically correct on WASIp1.

## Runner Runtime State

The runner now configures Wasmtime for:

- WebAssembly threads parsing;
- WebAssembly exceptions;
- shared linear memories;
- imported memories declared by the embedded module;
- WASIp1 imports from `wasmtime-wasi`;
- diagnostic host socket imports;
- trap handlers for remaining unsupported function imports.

The basic bundle checks pass:

```sh
./target/release/waasmtime-mysql --show-embedded-source
```

prints:

```text
build/mysql-wasi-port/build/runtime_output_directory/mysqld
```

A bounded runtime smoke test succeeds when guest `/tmp` is preopened:

```sh
tmpdir=$(mktemp -d)
mkdir -p "$tmpdir/tmp" "$tmpdir/data"
timeout 30s ./target/release/waasmtime-mysql \
  --no-default-preopen \
  --preopen "$tmpdir=/tmp" \
  --env TMPDIR=/tmp/tmp \
  --env HOME=/tmp \
  -- --no-defaults --help \
     --datadir=/tmp/data \
     --tmpdir=/tmp/tmp
```

It prints:

```text
mysqld  Ver 8.4.10 for WASI on wasm32 (Source distribution)
Starts the MySQL database server.
Usage: mysqld [OPTIONS]
```

Observed timings on this workspace:

```text
--help:          avg 3697 ms, min 3669 ms, max 3742 ms
--verbose --help avg 3687 ms, min 3667 ms, max 3709 ms
```

`--validate-config` also exits `0` with the same `/tmp` preopen. Real initialization and server startup reach the unsupported WASI threads import:

```text
error: embedded mysqld module trapped: error while executing at wasm backtrace:
    0: 0x1c7b9c7 - mysqld!__wasi_thread_spawn
    1: 0x1c8d031 - mysqld!__pthread_create
    2: 0x124b46e - mysqld!my_thread_create(...)
    3: 0x1735e25 - mysqld!pfs_spawn_thread_vc(...)
    4: 0x2e8b31 - mysqld!bootstrap::run_bootstrap_thread(...)
    5: 0x109aa08 - mysqld!dd::Dictionary_impl::init(...)
```

The previous 24 MiB fixed shared-memory import failed earlier with `std::bad_alloc` during static initialization. The explicit 128 MiB / 1 GiB memory limits move the first hard runtime blocker to thread spawning.

## Implication

There is now a single native executable that embeds the diagnostic MySQL `mysqld` WebAssembly module. It is not a working MySQL Server distribution.

The hard remaining work is runtime semantics, not just final linking:

- MySQL assumes a 64-bit platform, while the available WASIp1 target is 32-bit.
- WASI threads spawning is not implemented by this runner.
- The socket layer is a diagnostic host-import shim, not a complete validated MySQL networking port.
- Several libc, process, user, mmap, signal, and resource-group APIs are no-op or reduced-behavior shims.
- Startup paths that create pthreads trap on `wasi::thread-spawn`.

## Next Viable Work

- Implement or integrate WASI threads host support for `wasi::thread-spawn`, then test MySQL paths that create pthreads.
- Preopen or package MySQL support files such as `/share/english/errmsg.sys` so diagnostics are complete.
- Replace the diagnostic socket shim with a complete host-side socket adapter or a deliberate proxy design.
- Audit each libc/POSIX shim against MySQL startup and server workloads instead of treating link success as correctness.
- Evaluate a WASI Preview 2/component-model path with TCP socket support.
- Investigate a real wasm64 WASI C/C++ toolchain. The checked `wasi-sdk-33` image has an LLVM `wasm64` backend but no linkable wasm64 WASI sysroot for this build.
