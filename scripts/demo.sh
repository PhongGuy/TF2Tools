#!/usr/bin/env bash
# Opens TF2Tools on a seeded library and a fake TF2 install with throwaway state: `make demo`.
# Everything lives in ${XDG_CACHE_HOME:-~/.cache}/tf2tools-demo, rebuilt each run unless
# DEMO_KEEP=1. The stores stay online. DEMO_DRY_RUN=1 prints the seed and launch lines instead;
# TF2TOOLS_SEED names a built tf2tools-seed binary to skip cargo run.
# Must stay bash 3.2 safe, since macOS ships /bin/bash 3.2.
set -euo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cargo=${CARGO:-cargo}

die() { echo "demo: $*" >&2; exit 1; }

# XDG requires an absolute cache home; per the XDG spec, a relative one is ignored.
cache=${XDG_CACHE_HOME:-}
case $cache in
  /*) ;;
  *) cache=${HOME:?HOME is not set}/.cache ;;
esac
base=$cache/tf2tools-demo
# A fresh run deletes $base, so refuse anything that is not a real absolute path.
case $base in
  /?*/tf2tools-demo) ;;
  *) die "refusing demo directory $base" ;;
esac

# The app reads its library from XDG_DATA_HOME; the seeder saves the fake game as the TF2 path,
# which the app prefers over Steam detection, so installs only ever reach the fake tf/custom.
library=$base/data/com.tf2tools.app
game="$base/Team Fortress 2"
assets=$ROOT/assets
app=${CARGO_TARGET_DIR:-$ROOT/target}/release/tf2tools
launch_env="XDG_DATA_HOME=$base/data XDG_CONFIG_HOME=$base/config XDG_CACHE_HOME=$base/cache TF2TOOLS_ASSETS=$assets"
if [ -n "${DEMO_DRY_RUN:-}" ]; then
  echo "seed: tf2tools-seed $library $game"
  echo "$launch_env $app"
  exit 0
fi

if [ -z "${DEMO_KEEP:-}" ]; then
  rm -rf "$base"
fi
mkdir -p "$base/data" "$base/config" "$base/cache"

# Build and launch from the repo, so a relative CARGO_TARGET_DIR means the same dir to both.
cd "$ROOT"
# The pilot's seeder writes the library the QA screenshots use; it skips an already seeded one.
if [ -n "${TF2TOOLS_SEED:-}" ]; then
  TF2TOOLS_ASSETS=$assets "$TF2TOOLS_SEED" "$library" "$game"
else
  TF2TOOLS_ASSETS=$assets "$cargo" run --quiet --manifest-path tools/tf2tools-pilot/Cargo.toml \
    --bin tf2tools-seed -- "$library" "$game"
fi

# A release build: a debug GPUI cannot hold frame rate on a high-refresh display.
"$cargo" build --release -p tf2tools-app
exec env XDG_DATA_HOME="$base/data" XDG_CONFIG_HOME="$base/config" XDG_CACHE_HOME="$base/cache" \
  TF2TOOLS_ASSETS="$assets" "$app"
