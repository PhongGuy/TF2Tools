#!/usr/bin/env bash
# Lints the workspace for Windows, or links a debug tf2tools.exe, from Linux: the host's Rust
# toolchain runs read-only inside an ubuntu:24.04 image that adds the mingw-w64 C toolchain.
# CI builds the real msvc binaries on windows-2025; this catches compile errors before a push.
set -euo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
TARGET=x86_64-pc-windows-gnu
IMAGE=localhost/tf2tools-wincheck:1
mode=${1:-lint}
case $mode in
  lint) cmd=(cargo clippy --locked --keep-going --target "$TARGET" --workspace --all-targets --all-features -- -Dwarnings) ;;
  build) cmd=(cargo build --locked --target "$TARGET" -p tf2tools-app --bin tf2tools) ;;
  *) echo "usage: scripts/check-windows.sh [lint|build]" >&2; exit 2 ;;
esac

engine=$(command -v podman || command -v docker || true)
[ -n "$engine" ] || { echo "FAIL: needs podman or docker" >&2; exit 1; }
sysroot=$(rustc --print sysroot)
[ -d "$sysroot/lib/rustlib/$TARGET" ] || { echo "FAIL: run rustup target add $TARGET" >&2; exit 1; }
cargo_home=${CARGO_HOME:-$HOME/.cargo}
target_dir=${CARGO_TARGET_DIR:-$ROOT/target}
mkdir -p "$target_dir"

# The image holds only the C cross toolchain, built once from an empty context.
if ! "$engine" image inspect "$IMAGE" > /dev/null 2>&1; then
  context=$(mktemp -d)
  trap 'rm -rf "$context"' EXIT
  cat > "$context/Containerfile" << 'EOF'
FROM docker.io/library/ubuntu:24.04
RUN apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
      gcc libc6-dev gcc-mingw-w64-x86-64 g++-mingw-w64-x86-64 binutils-mingw-w64-x86-64 cmake ca-certificates \
    && rm -rf /var/lib/apt/lists/*
EOF
  "$engine" build -t "$IMAGE" -f "$context/Containerfile" "$context"
fi

# Rootless podman maps the container's root to this user; docker needs the user named.
user=()
case $engine in *docker) user=(--user "$(id -u):$(id -g)") ;; esac
"$engine" run --rm "${user[@]}" \
  -v "$ROOT":/src:ro -v "$sysroot":/toolchain:ro -v "$cargo_home":/cargo -v "$target_dir":/target \
  -e CARGO_HOME=/cargo -e CARGO_TARGET_DIR=/target -e PATH=/toolchain/bin:/usr/bin:/bin \
  -e CARGO_PROFILE_DEV_DEBUG=0 -e CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-4}" -e CARGO_TERM_COLOR=never \
  -e AWS_LC_SYS_PREBUILT_NASM=1 \
  -w /src "$IMAGE" "${cmd[@]}"
