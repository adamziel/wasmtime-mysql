#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source_dir="${MYSQL_SOURCE:-$root/third_party/mysql-server}"
build_dir="${MYSQL_WASI_PORT_BUILD_DIR:-$root/build/mysql-wasi-port}"
src_dir="$build_dir/src"
cmake_build_dir="$build_dir/build"
host_tools_dir="$build_dir/host-tools"
protobuf_wrapper_dir="$build_dir/protobuf-host-wrapper"
openssl_prefix="${OPENSSL_WASI_PREFIX:-$root/build/openssl-wasi/install}"
image="${WASI_SDK_IMAGE:-ghcr.io/webassembly/wasi-sdk:wasi-sdk-33}"
bison_image="${BISON_IMAGE:-debian:bookworm-slim}"
patch_file="$root/patches/mysql-wasi/0001-add-wasi-port-probe-cmake-bypasses.patch"
lifecycle_patch_file="$root/patches/mysql-wasi/0002-wasi-graceful-shutdown.patch"
log_file="${MYSQL_WASI_PORT_PROBE_LOG:-$build_dir/probe.log}"
target="${MYSQL_WASI_PORT_TARGET:-mysqld}"

if [[ ! -f "$source_dir/CMakeLists.txt" ]]; then
  MYSQL_SOURCE="$source_dir" "$root/scripts/fetch-mysql-source.sh"
fi

if [[ ! -f "$openssl_prefix/lib/libssl.a" || ! -f "$openssl_prefix/lib/libcrypto.a" ]]; then
  OPENSSL_WASI_BUILD_DIR="$(dirname "$openssl_prefix")" "$root/scripts/build-openssl-wasi.sh"
fi

mkdir -p "$build_dir"
docker run --rm \
  -v "$build_dir:/build:z" \
  "$image" \
  sh -c 'rm -rf /build/* /build/.[!.]* /build/..?*'

mkdir -p "$src_dir" "$cmake_build_dir" "$host_tools_dir" "$protobuf_wrapper_dir"
git -C "$source_dir" archive HEAD | tar -x -C "$src_dir"
cp "$root/patches/mysql-wasi/files/protobuf-host-wrapper/CMakeLists.txt" \
  "$protobuf_wrapper_dir/CMakeLists.txt"
cp "$root/patches/mysql-wasi/files/include/mysql_wasi_socket_shim.h" \
  "$src_dir/include/mysql_wasi_socket_shim.h"
cp "$root/patches/mysql-wasi/files/include/mysql_wasi_libc_shim.h" \
  "$src_dir/include/mysql_wasi_libc_shim.h"
cp "$root/patches/mysql-wasi/files/include/mysql_wasi_runtime_shim.h" \
  "$src_dir/include/mysql_wasi_runtime_shim.h"
cp "$root/patches/mysql-wasi/files/include/syslog.h" \
  "$src_dir/include/syslog.h"
mkdir -p "$src_dir/include/sys"
cp "$root/patches/mysql-wasi/files/include/sys/resource.h" \
  "$src_dir/include/sys/resource.h"
cp "$root/patches/mysql-wasi/files/include/sys/times.h" \
  "$src_dir/include/sys/times.h"
cp "$root/patches/mysql-wasi/files/include/netdb.h" \
  "$src_dir/include/netdb.h"
cp "$root/patches/mysql-wasi/files/include/pwd.h" \
  "$src_dir/include/pwd.h"
cp "$root/patches/mysql-wasi/files/vio/mysql_wasi_socket_shim.c" \
  "$src_dir/vio/mysql_wasi_socket_shim.c"
git apply --directory="${src_dir#$root/}" "$patch_file"
git apply --directory="${src_dir#$root/}" "$lifecycle_patch_file"
: > "$log_file"

if ! docker run --rm \
	  -v "$src_dir:/mysql:z" \
	  -v "$host_tools_dir:/host-tools:z" \
	  -v "$protobuf_wrapper_dir:/protobuf-host-wrapper:ro,z" \
	  -w /mysql/sql \
	  "$bison_image" \
  sh -euxc '
    apt-get update
    DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
      bison \
      ca-certificates \
      cmake \
      g++ \
      libicu-dev \
      libncurses-dev \
      libssl-dev \
      libtirpc-dev \
      ninja-build \
      pkg-config \
      zlib1g-dev
    bison --yacc --warnings=all,no-yacc,no-precedence --no-lines \
      --defines=sql_yacc.h \
      --output=sql_yacc.cc \
      sql_yacc.yy
    bison --yacc --warnings=all,no-yacc,no-precedence --no-lines \
      --defines=sql_hints.yy.h \
      --output=sql_hints.yy.cc \
      sql_hints.yy
    mkdir -p /tmp/mysql-host-include
    printf "#pragma once\n#define HAVE_STRNLEN 1\n" > /tmp/mysql-host-include/my_config.h
    g++ -std=c++20 \
      -I/tmp/mysql-host-include \
      -I/mysql \
      -I/mysql/include \
      -I/mysql/strings \
      -o /host-tools/uca9dump \
      /mysql/strings/uca9-dump.cc
    g++ -std=c++20 \
      -I/tmp/mysql-host-include \
      -I/mysql \
      -I/mysql/include \
      -I/mysql/sql \
      -o /host-tools/gen_lex_hash \
      /mysql/sql/gen_lex_hash.cc
    g++ -std=c++20 \
      -I/tmp/mysql-host-include \
      -I/mysql \
      -I/mysql/include \
      -I/mysql/sql \
      -o /host-tools/gen_lex_token \
      /mysql/sql/gen_lex_token.cc
    g++ -std=c++20 \
      -I/tmp/mysql-host-include \
      -I/mysql \
      -I/mysql/include \
      -I/mysql/sql \
      $(pkg-config --cflags icu-uc icu-i18n) \
      -o /host-tools/gen_keyword_list \
      /mysql/sql/gen_keyword_list.cc \
      $(pkg-config --libs icu-uc icu-i18n)
    mkdir -p /host-tools/lib
    ldd /host-tools/gen_keyword_list \
      | awk "/libicu/ {print \$3}" \
      | xargs -r cp -L -t /host-tools/lib
    cmake -S /mysql -B /host-tools/native-build -GNinja \
      -DCMAKE_BUILD_TYPE=Release \
      -DWITHOUT_SERVER=ON \
      -DWITH_UNIT_TESTS=OFF \
      -DWITH_SSL=system
	    cmake --build /host-tools/native-build --target comp_err comp_client_err comp_sql
	    cp /host-tools/native-build/runtime_output_directory/comp_err /host-tools/comp_err
	    cp /host-tools/native-build/runtime_output_directory/comp_client_err /host-tools/comp_client_err
	    cp /host-tools/native-build/runtime_output_directory/comp_sql /host-tools/comp_sql
	    cmake -S /protobuf-host-wrapper -B /host-tools/protobuf-native -GNinja \
	      -DCMAKE_BUILD_TYPE=Release \
	      -DPROTOBUF_SOURCE_DIR=/mysql/extra/protobuf/protobuf-24.4 \
	      -Dprotobuf_BUILD_TESTS=OFF \
	      -Dprotobuf_BUILD_CONFORMANCE=OFF \
	      -Dprotobuf_BUILD_EXAMPLES=OFF \
	      -Dprotobuf_BUILD_SHARED_LIBS=OFF \
	      -Dprotobuf_WITH_ZLIB=OFF \
	      -DABSL_ROOT_DIR=/mysql/extra/abseil/abseil-cpp-20230802.1
	    cmake --build /host-tools/protobuf-native --target protoc
	    cp -L /host-tools/protobuf-native/runtime_output_directory/protoc /host-tools/protoc
	  ' >> "$log_file" 2>&1; then
  tail -n 160 "$log_file"
  exit 1
fi

if ! docker run --rm \
  -e MYSQL_WASI_PORT_TARGET="$target" \
  -v "$src_dir:/mysql:ro,z" \
  -v "$cmake_build_dir:/build:z" \
  -v "$host_tools_dir:/host-tools:ro,z" \
  -v "$openssl_prefix:/openssl-wasi:ro,z" \
  -w /build \
  "$image" \
  sh -euxc '
    wasi_sdk_path="${WASI_SDK_PATH:-/opt/wasi-sdk}"
    toolchain="$wasi_sdk_path/share/cmake/wasi-sdk-pthread.cmake"
    if [ ! -f "$toolchain" ]; then
      toolchain="$(find / -path "*/share/cmake/wasi-sdk-pthread.cmake" -print -quit)"
    fi
    test -n "$toolchain"

	    export PATH="/host-tools:$PATH"

    cmake -S /mysql -B /build -GNinja \
      -DCMAKE_TOOLCHAIN_FILE="$toolchain" \
      -DCMAKE_BUILD_TYPE=Release \
      -DCMAKE_CXX_STANDARD=20 \
      -DCMAKE_CXX_STANDARD_REQUIRED=ON \
      -DCMAKE_TRY_COMPILE_TARGET_TYPE=STATIC_LIBRARY \
      -DCMAKE_C_FLAGS="-include /mysql/include/mysql_wasi_libc_shim.h -D_WASI_EMULATED_MMAN -D_WASI_EMULATED_SIGNAL -DZ_HAVE_UNISTD_H -DU_HAVE_TZSET=0 -DU_HAVE_TIMEZONE=0 -DU_HAVE_TZNAME=0 -DU_HAVE_MMAP=0 -DU_HAVE_POPEN=0 -DU_ENABLE_DYLOAD=0" \
      -DCMAKE_CXX_FLAGS="-include /mysql/include/mysql_wasi_libc_shim.h -D_WASI_EMULATED_MMAN -D_WASI_EMULATED_SIGNAL -DZ_HAVE_UNISTD_H -DU_HAVE_TZSET=0 -DU_HAVE_TIMEZONE=0 -DU_HAVE_TZNAME=0 -DU_HAVE_MMAP=0 -DU_HAVE_POPEN=0 -DU_ENABLE_DYLOAD=0" \
      -DCMAKE_EXE_LINKER_FLAGS="-fwasm-exceptions -lwasi-emulated-getpid -lwasi-emulated-mman -lwasi-emulated-signal -lunwind -Wl,--initial-memory=134217728 -Wl,--max-memory=1073741824" \
      -DCMAKE_SHARED_LINKER_FLAGS="-fwasm-exceptions -lwasi-emulated-getpid -lwasi-emulated-mman -lwasi-emulated-signal -lunwind -Wl,--initial-memory=134217728 -Wl,--max-memory=1073741824" \
      -DHAVE_CLOCK_GETTIME_EXITCODE=0 \
      -DHAVE_CLOCK_REALTIME_EXITCODE=0 \
      -DHAVE___BUILTIN_FFS_EXITCODE=0 \
      -DFORCE_UNSUPPORTED_COMPILER=ON \
      -DMYSQL_WASI_PORT_PROBE=ON \
      -DTMPDIR=/tmp \
      -DDOWNLOAD_BOOST=1 \
      -DWITH_BOOST=/build/boost \
      -DUSE_BISON_RESULTS_FROM_MAKE_DIST=ON \
      -DWITH_UNIT_TESTS=OFF \
      -DWITH_ROUTER=OFF \
      -DWITH_NDB=OFF \
      -DWITH_NDBCLUSTER=OFF \
      -DWITH_NDBCLUSTER_STORAGE_ENGINE=OFF \
      -DWITHOUT_ARCHIVE_STORAGE_ENGINE=ON \
      -DWITHOUT_BLACKHOLE_STORAGE_ENGINE=ON \
      -DWITHOUT_EXAMPLE_STORAGE_ENGINE=ON \
      -DWITHOUT_FEDERATED_STORAGE_ENGINE=ON \
      -DWITHOUT_MOCK_SECONDARY_STORAGE_ENGINE=ON \
      -DWITH_MYSQLX=OFF \
      -DWITH_GROUP_REPLICATION=OFF \
      -DWITH_AUTHENTICATION_CLIENT_PLUGINS=OFF \
      -DWITH_AUTHENTICATION_KERBEROS=OFF \
      -DWITH_AUTHENTICATION_LDAP=OFF \
      -DWITH_AUTHENTICATION_WEBAUTHN=OFF \
      -DWITH_FIDO=none \
      -DWITH_CURL=none \
      -DWITH_SSL=system \
      -DOPENSSL_ROOT_DIR=/openssl-wasi \
      -DOPENSSL_INCLUDE_DIR=/openssl-wasi/include \
      -DOPENSSL_SSL_LIBRARY=/openssl-wasi/lib/libssl.a \
      -DOPENSSL_CRYPTO_LIBRARY=/openssl-wasi/lib/libcrypto.a \
      -DOPENSSL_USE_STATIC_LIBS=TRUE \
      -Dprotobuf_BUILD_SHARED_LIBS=OFF \
      -Dprotobuf_BUILD_PROTOC_BINARIES=OFF \
      -Dprotobuf_BUILD_LIBPROTOC=OFF \
	      -DWITH_PROTOC=/host-tools/protoc \
      -DUCA9DUMP_EXECUTABLE=/host-tools/uca9dump \
      -DCOMP_ERR_EXECUTABLE=/host-tools/comp_err \
      -DCOMP_CLIENT_ERR_EXECUTABLE=/host-tools/comp_client_err \
      -DGEN_LEX_HASH_EXECUTABLE=/host-tools/gen_lex_hash \
      -DGEN_LEX_TOKEN_EXECUTABLE=/host-tools/gen_lex_token \
      -DGEN_KEYWORD_LIST_EXECUTABLE=/host-tools/gen_keyword_list \
      -DWITH_ZLIB=bundled \
      -DWITH_ZSTD=bundled \
      -DWITH_LZ4=bundled \
      -DWITH_ICU=bundled \
      -DWITH_LIBEVENT=bundled

    cmake --build /build --target "$MYSQL_WASI_PORT_TARGET"
  ' > "$log_file" 2>&1; then
  tail -n 160 "$log_file"
  exit 1
fi

printf 'MySQL WASI port probe built target: %s\n' "$target"
printf 'Build directory: %s\n' "$cmake_build_dir"
