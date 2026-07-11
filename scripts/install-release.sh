#!/usr/bin/env sh
set -eu

VERSION="${VERSION:-v0.1.10}"
REPO="${REPO:-adamziel/wasmtime-mysql}"
BASE_URL="https://github.com/$REPO/releases/download/$VERSION"

case "$(uname -s)-$(uname -m)" in
  Linux-x86_64) ASSET=linux-x86_64 ;;
  Darwin-x86_64) ASSET=macos-x86_64 ;;
  Darwin-arm64) ASSET=macos-aarch64 ;;
  *)
    echo "unsupported platform: $(uname -s)-$(uname -m)" >&2
    exit 1
    ;;
esac

ARCHIVE="wasmtime-mysql-$VERSION-$ASSET.tar.gz"
DIR="wasmtime-mysql-$VERSION-$ASSET"

curl -fL -o "$ARCHIVE" "$BASE_URL/$ARCHIVE"
curl -fL -o SHA256SUMS "$BASE_URL/SHA256SUMS"

CHECKSUM_LINE="$(awk -v file="$ARCHIVE" '$2 == file { print }' SHA256SUMS)"
if [ -z "$CHECKSUM_LINE" ]; then
  echo "checksum for $ARCHIVE not found in SHA256SUMS" >&2
  exit 1
fi

if command -v sha256sum >/dev/null 2>&1; then
  printf '%s\n' "$CHECKSUM_LINE" | sha256sum -c -
else
  printf '%s\n' "$CHECKSUM_LINE" | shasum -a 256 -c -
fi

tar -xzf "$ARCHIVE"

cat <<EOF

Downloaded and verified $ARCHIVE.
Next:
  cd "$DIR"
EOF
