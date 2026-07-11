#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
runner="${MYSQL_RUNNER:-$root/target/release/wasmtime-mysql}"
mysql_client="${MYSQL_CLIENT:-$root/build/mysql-wasi-port/host-tools/native-build/runtime_output_directory/mysql}"
port="${MYSQL_PORT:-3337}"
run_dir="$(mktemp -d "${TMPDIR:-/tmp}/wasmtime-mysql-lifecycle.XXXXXX")"
server_pid=""
competing_pid=""

cleanup() {
  local status=$?
  trap - EXIT
  if [[ -n "$server_pid" ]] && kill -0 "$server_pid" 2>/dev/null; then
    kill -KILL "$server_pid" 2>/dev/null || true
    wait "$server_pid" 2>/dev/null || true
  fi
  if [[ -n "$competing_pid" ]] && kill -0 "$competing_pid" 2>/dev/null; then
    kill -KILL "$competing_pid" 2>/dev/null || true
    wait "$competing_pid" 2>/dev/null || true
  fi
  rm -rf "$run_dir"
  exit "$status"
}
trap cleanup EXIT

if [[ ! -x "$runner" ]]; then
  echo "runner is not executable: $runner" >&2
  exit 2
fi

if [[ ! -x "$mysql_client" ]]; then
  echo "MySQL 8.4 client is not executable: $mysql_client" >&2
  exit 2
fi

mysql() {
  "$mysql_client" --protocol=TCP -h127.0.0.1 -P"$port" -uroot \
    --ssl-mode=DISABLED "$@"
}

dump_server_log() {
  local name="$1"
  for log in "$run_dir/$name.out" "$run_dir/$name.err"; do
    if [[ -f "$log" ]]; then
      echo "--- $log" >&2
      tail -n 120 "$log" >&2
    fi
  done
}

server_log_contains() {
  local name="$1"
  local needle="$2"
  grep -Fq "$needle" "$run_dir/$name.out" "$run_dir/$name.err" 2>/dev/null
}

require_server_log_contains() {
  local name="$1"
  local needle="$2"
  if ! server_log_contains "$name" "$needle"; then
    dump_server_log "$name"
    echo "server log is missing: $needle" >&2
    exit 1
  fi
}

start_server() {
  local name="$1"
  local attempts=0

  "$runner" \
    --no-default-preopen \
    --preopen "$run_dir=/tmp" \
    --env TMPDIR=/tmp/tmp \
    --env HOME=/tmp \
    -- \
    --no-defaults \
    --console \
    --datadir=/tmp/data \
    --tmpdir=/tmp/tmp \
    --log-error="/tmp/$name.err" \
    --log-error-verbosity=3 \
    --port="$port" \
    --bind-address=127.0.0.1 \
    --skip-log-bin \
    --auto-generate-certs=OFF \
    --sha256-password-auto-generate-rsa-keys=OFF \
    --caching-sha2-password-auto-generate-rsa-keys=OFF \
    >"$run_dir/$name.out" 2>&1 &
  server_pid=$!

  while ((attempts < 120)); do
    if mysql -Nse 'SELECT 1' >/dev/null 2>&1; then
      return
    fi
    if ! kill -0 "$server_pid" 2>/dev/null; then
      wait "$server_pid" || true
      server_pid=""
      dump_server_log "$name"
      echo "server exited before it accepted connections" >&2
      exit 1
    fi
    sleep 0.25
    ((attempts += 1))
  done

  dump_server_log "$name"
  echo "server did not accept connections on port $port" >&2
  exit 1
}

start_competing_server() {
  local name="$1"
  local competing_port=$((port + 1))

  "$runner" \
    --no-default-preopen \
    --preopen "$run_dir=/tmp" \
    --env TMPDIR=/tmp/tmp \
    --env HOME=/tmp \
    -- \
    --no-defaults \
    --console \
    --datadir=/tmp/data \
    --tmpdir=/tmp/tmp \
    --log-error="/tmp/$name.err" \
    --log-error-verbosity=3 \
    --port="$competing_port" \
    --bind-address=127.0.0.1 \
    --skip-log-bin \
    --auto-generate-certs=OFF \
    --sha256-password-auto-generate-rsa-keys=OFF \
    --caching-sha2-password-auto-generate-rsa-keys=OFF \
    >"$run_dir/$name.out" 2>&1 &
  competing_pid=$!
}

wait_for_server_exit() {
  local name="$1"
  local attempts=0

  while kill -0 "$server_pid" 2>/dev/null; do
    if ((attempts >= 120)); then
      dump_server_log "$name"
      echo "server did not exit after shutdown" >&2
      exit 1
    fi
    sleep 0.25
    ((attempts += 1))
  done

  if ! wait "$server_pid"; then
    dump_server_log "$name"
    echo "server exited unsuccessfully" >&2
    exit 1
  fi
  server_pid=""
}

mkdir -p "$run_dir/tmp"
"$runner" \
  --no-default-preopen \
  --preopen "$run_dir=/tmp" \
  --env TMPDIR=/tmp/tmp \
  --env HOME=/tmp \
  -- \
  --no-defaults \
  --initialize-insecure \
  --skip-networking \
  --console \
  --datadir=/tmp/data \
  --tmpdir=/tmp/tmp \
  --log-error=/tmp/init.err \
  --log-error-verbosity=3 \
  --auto-generate-certs=OFF \
  --sha256-password-auto-generate-rsa-keys=OFF \
  --caching-sha2-password-auto-generate-rsa-keys=OFF \
  >"$run_dir/init.out" 2>&1

start_server sql-shutdown
mysql -e "CREATE DATABASE lifecycle; CREATE TABLE lifecycle.t (id INT PRIMARY KEY, value_text VARCHAR(32)) ENGINE=InnoDB; INSERT INTO lifecycle.t VALUES (1, 'durable'); SHUTDOWN"
wait_for_server_exit sql-shutdown
require_server_log_contains sql-shutdown 'MySQL Server: Closing Connections - end.'

start_server sigint-shutdown
[[ "$(mysql -Nse 'SELECT value_text FROM lifecycle.t WHERE id = 1')" == "durable" ]]
kill -INT "$server_pid"
wait_for_server_exit sigint-shutdown
require_server_log_contains sigint-shutdown 'Dumping buffer pool(s)'

start_server sigterm-shutdown
[[ "$(mysql -Nse 'SELECT value_text FROM lifecycle.t WHERE id = 1')" == "durable" ]]
kill -TERM "$server_pid"
wait_for_server_exit sigterm-shutdown
require_server_log_contains sigterm-shutdown 'Dumping buffer pool(s)'

start_server clean-restart
[[ "$(mysql -Nse 'SELECT value_text FROM lifecycle.t WHERE id = 1')" == "durable" ]]
if server_log_contains clean-restart 'Database was not shutdown normally!'; then
  dump_server_log clean-restart
  echo "clean shutdown unexpectedly required InnoDB crash recovery" >&2
  exit 1
fi
mysql -e 'SHUTDOWN'
wait_for_server_exit clean-restart

start_server datadir-lock-primary
start_competing_server datadir-lock-secondary
for _ in $(seq 1 60); do
  if server_log_contains datadir-lock-secondary 'Unable to lock ./ibdata1'; then
    break
  fi
  sleep 0.25
done
require_server_log_contains datadir-lock-secondary 'Unable to lock ./ibdata1'
kill -KILL "$competing_pid" 2>/dev/null || true
wait "$competing_pid" 2>/dev/null || true
competing_pid=""
mysql -e 'SHUTDOWN'
wait_for_server_exit datadir-lock-primary

echo "lifecycle test passed"
