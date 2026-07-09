#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
wasm="${1:-${MYSQLD_WASM:-}}"

if [[ -z "$wasm" ]]; then
  echo "usage: $0 /absolute/or/relative/path/to/mysqld.wasm" >&2
  echo "or set MYSQLD_WASM before running this script" >&2
  exit 2
fi

if [[ ! -f "$wasm" ]]; then
  echo "mysqld wasm module not found: $wasm" >&2
  exit 2
fi

cd "$root"
MYSQLD_WASM="$wasm" cargo build --release
ls -lh "$root/target/release/waasmtime-mysql"
