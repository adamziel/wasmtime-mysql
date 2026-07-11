#ifndef MYSQL_WASI_RUNTIME_SHIM_H
#define MYSQL_WASI_RUNTIME_SHIM_H

#if defined(__wasi__)

#ifdef __cplusplus
extern "C" {
#endif

__attribute__((import_module("wasmtime_mysql_runtime"),
               import_name("request_shutdown")))
void waasmtime_mysql_request_shutdown(void);

__attribute__((import_module("wasmtime_mysql_runtime"),
               import_name("shutdown_requested")))
int waasmtime_mysql_shutdown_requested(void);

#ifdef __cplusplus
}
#endif

#endif /* __wasi__ */

#endif /* MYSQL_WASI_RUNTIME_SHIM_H */
