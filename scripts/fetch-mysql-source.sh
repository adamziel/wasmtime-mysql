#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
mysql_ref="${MYSQL_REF:-mysql-8.4.10}"
dest="${MYSQL_SOURCE:-$root/third_party/mysql-server}"

mkdir -p "$(dirname "$dest")"

if [[ -d "$dest/.git" ]]; then
  git -C "$dest" fetch --depth 1 origin "refs/tags/$mysql_ref:refs/tags/$mysql_ref"
  git -C "$dest" checkout --detach "$mysql_ref"
else
  git clone --depth 1 --branch "$mysql_ref" https://github.com/mysql/mysql-server.git "$dest"
fi

git -C "$dest" rev-parse HEAD
