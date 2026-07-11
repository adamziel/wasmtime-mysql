# Status

## What Works Here

- The port builds MySQL `8.4.10` (`6adc159923b7b6abbe649949551ec25264c2daf9`)
  as a WASI threads module and embeds it in a native Wasmtime runner.
- A server can initialize a fresh datadir, listen on TCP, accept concurrent
  clients, create InnoDB tables, and run normal SQL.
- InnoDB file writes use native host positional I/O and `sync_data` or
  `sync_all`; directory metadata sync uses a host directory `fsync`.
- The host uses POSIX nonblocking record locks for InnoDB data files. A second
  runner using the same datadir fails to lock `ibdata1` instead of starting.
- SQL `SHUTDOWN`, `SIGINT`, and `SIGTERM` take a graceful path. The lifecycle
  regression verifies server exit, row durability across restart, and no
  crash-recovery message after a clean stop.

Run the source-build regression with:

```sh
./scripts/test-lifecycle.sh
```

The test expects the native MySQL 8.4 client produced by
`scripts/probe-mysql-wasi-port.sh`; set `MYSQL_CLIENT` to override it.

## How It Works

The runner enables Wasmtime shared memory, exceptions, and WebAssembly threads.
Each guest pthread starts in a fresh Wasm instance but imports the same shared
linear memory. The host also supplies narrow imports for sockets, file I/O,
file synchronization, directory synchronization, data-file locking, and the
shutdown control flag.

The guest signal thread is intentionally bypassed. MySQL's normal Unix design
uses signals and a joinable signal-handler thread; that is not reliable through
the current WASI thread ABI. The host flag wakes the TCP listener within 100 ms
and lets MySQL's main thread run its ordinary connection and storage-engine
shutdown work.

## Still Not a Production Port

- MySQL's upstream platform check is bypassed for a `wasm32` target even though
  upstream MySQL expects a 64-bit platform.
- The WASI build reports no native memory-barrier capability. Mutex operations
  are real shared-memory operations, but native MySQL memory-ordering
  assumptions have not been proven for all schedules.
- Binary logging is unsupported. The GTID compression worker is disabled in
  this WASI build because its join path is unsafe.
- TLS, RSA key generation, dynamic component loading, and packaged
  `errmsg.sys` support are incomplete or intentionally disabled.
- This has not had power-loss fault injection, a complete upstream MTR run, or
  long-duration high-contention validation. Treat crash recovery and unusual
  lifecycle failures as areas that still need deliberate testing.
