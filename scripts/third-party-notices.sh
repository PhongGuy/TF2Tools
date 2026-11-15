#!/usr/bin/env bash
# Regenerates licenses/THIRD-PARTY-NOTICES.txt from Cargo.lock with cargo-about, using about.toml's
# allow list and the about.hbs template. With --check it fails instead when the committed file is stale.
# Usage: third-party-notices.sh [--check]   (CARGO_ABOUT names the binary, default cargo-about)
set -euo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
ABOUT=${CARGO_ABOUT:-cargo-about}
VERSION=0.9.3
OUT=$ROOT/licenses/THIRD-PARTY-NOTICES.txt

check=0
case ${1:-} in
  "") ;;
  --check) check=1 ;;
  *) echo "usage: third-party-notices.sh [--check]" >&2; exit 2 ;;
esac

# Another cargo-about version words and orders the file differently, so only the pinned one may write it.
found=$("$ABOUT" --version 2> /dev/null || true)
[ "$found" = "cargo-about $VERSION" ] || {
  echo "needs cargo-about $VERSION (found: ${found:-none}): cargo install --locked cargo-about --version $VERSION --features cli" >&2
  exit 1
}

# Offline after one fetch, so the text comes only from the crates' own files and never from the network.
cargo fetch --locked --manifest-path "$ROOT/Cargo.toml" > /dev/null
fresh=$(mktemp)
trap 'rm -f "$fresh"' EXIT
"$ABOUT" generate --frozen --fail -c "$ROOT/about.toml" -m "$ROOT/crates/tf2tools-app/Cargo.toml" \
  -o "$fresh" "$ROOT/about.hbs"

if [ "$check" -eq 1 ]; then
  diff -u "$OUT" "$fresh" > /dev/null || {
    echo "licenses/THIRD-PARTY-NOTICES.txt is stale: run scripts/third-party-notices.sh and commit it" >&2
    diff -u "$OUT" "$fresh" | head -40 >&2 || true
    exit 1
  }
  echo "third-party notices are current"
else
  cp "$fresh" "$OUT"
fi
