#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source_dir="${MYSQL_SOURCE:-$root/third_party/mysql-server}"
build_dir="${MYSQL_WASI_BUILD_DIR:-$root/build/mysql-wasi}"
image="${WASI_SDK_IMAGE:-ghcr.io/webassembly/wasi-sdk:wasi-sdk-33}"
log_file="${MYSQL_WASI_PROBE_LOG:-$build_dir/probe.log}"

if [[ ! -f "$source_dir/CMakeLists.txt" ]]; then
  MYSQL_SOURCE="$source_dir" "$root/scripts/fetch-mysql-source.sh"
fi

mkdir -p "$build_dir"
: > "$log_file"

docker run --rm \
  -v "$source_dir:/mysql:ro,z" \
  -v "$build_dir:/build:z" \
  -w /build \
  "$image" \
  sh -euxc '
    wasi_sdk_path="${WASI_SDK_PATH:-/opt/wasi-sdk}"
    toolchain="$wasi_sdk_path/share/cmake/wasi-sdk-pthread.cmake"
    if [ ! -f "$toolchain" ]; then
      toolchain="$(find / -path "*/share/cmake/wasi-sdk-pthread.cmake" -print -quit)"
    fi
    test -n "$toolchain"

    cmake -S /mysql -B /build -GNinja \
      -DCMAKE_TOOLCHAIN_FILE="$toolchain" \
      -DCMAKE_BUILD_TYPE=Release \
      -DCMAKE_TRY_COMPILE_TARGET_TYPE=STATIC_LIBRARY \
      -DFORCE_UNSUPPORTED_COMPILER=ON \
      -DDOWNLOAD_BOOST=1 \
      -DWITH_BOOST=/build/boost \
      -DWITH_UNIT_TESTS=OFF \
      -DWITH_ROUTER=OFF \
      -DWITH_NDB=OFF \
      -DWITH_ZLIB=bundled \
      -DWITH_ZSTD=bundled \
      -DWITH_LZ4=bundled \
      -DWITH_ICU=bundled \
      -DWITH_LIBEVENT=bundled

    cmake --build /build --target mysqld
  ' 2>&1 | tee "$log_file"
