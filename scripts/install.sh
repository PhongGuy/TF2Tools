#!/usr/bin/env bash
# Installs or uninstalls TF2Tools for the current user on Linux: `install.sh install|uninstall`.
# TF2TOOLS_PREFIX (default ~/.local) gets bin/tf2tools, a link into lib/tf2tools where the binary
# sits beside its assets. TF2TOOLS_BIN names a prebuilt binary to skip cargo; JOBS caps the build;
# TF2TOOLS_V5_PROCESS overrides the process name checked before backing up a v5 library.
# Must stay bash 3.2 safe, since macOS ships /bin/bash 3.2.
set -euo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cargo=${CARGO:-cargo}
action=${1:-}

die() { echo "install: $*" >&2; exit 1; }

case $action in
  install | uninstall) ;;
  *) die "usage: install.sh install|uninstall" ;;
esac
[ "$(uname -s)" = Linux ] || die "only Linux is supported until the release packages exist"

prefix=${TF2TOOLS_PREFIX:-$HOME/.local}
case $prefix in /*) ;; *) prefix=$PWD/$prefix ;; esac
# XDG requires an absolute data home; per the XDG spec, a relative one is ignored.
data=${XDG_DATA_HOME:-}
case $data in /*) ;; *) data=$HOME/.local/share ;; esac

libdir=$prefix/lib/tf2tools
link=$prefix/bin/tf2tools
desktop=$data/applications/com.tf2tools.app.desktop
icon_sizes="32x32:32x32.png 128x128:128x128.png 256x256:128x128@2x.png 512x512:icon.png"
library=$data/com.tf2tools.app
db=$library/tf2tools.db
v5_process=${TF2TOOLS_V5_PROCESS:-tf2tools}

# Uninstall and reinstall delete $libdir wholesale, so refuse anything but a real prefix.
case $libdir in
  /?*/lib/tf2tools) ;;
  *) die "refusing install directory $libdir" ;;
esac

# Cache refreshers are optional and their failure never fails the install.
refresh_caches() {
  if command -v update-desktop-database > /dev/null 2>&1; then
    update-desktop-database -q "$data/applications" 2> /dev/null || true
  fi
  if command -v gtk-update-icon-cache > /dev/null 2>&1 && [ -f "$data/icons/hicolor/icon-theme.cache" ]; then
    gtk-update-icon-cache -q -t "$data/icons/hicolor" 2> /dev/null || true
  fi
}

# SQLite keeps PRAGMA user_version as a big-endian u32 at byte 60 of the header.
schema_version() {
  od -An -tu1 -j60 -N4 "$1" | awk '{ v = 0; for (i = 1; i <= NF; i++) v = v * 256 + $i; print v + 0 }'
}

# One awk reads the whole list, so no writer dies of SIGPIPE under pipefail.
running() {
  ps -A -o comm= | awk -F/ -v name="$v5_process" '$NF == name { found = 1 } END { exit !found }'
}

# v6 upgrades a v5 library to schema 10 on its first launch; keep a copy as a precaution.
# Only copies: the originals are never moved, edited or deleted.
backup_v5_library() {
  [ -f "$db" ] || return 0
  local version
  version=$(schema_version "$db")
  if [ "${version:-0}" -ge 10 ]; then
    return 0
  fi
  if running; then
    die "TF2Tools ($v5_process) is running; close it so its library can be backed up, then retry"
  fi
  local existing
  existing=$(find "$library" -maxdepth 1 -type d -name 'backup-v5-*' | head -n 1)
  if [ -n "$existing" ]; then
    echo "A v5 library backup already exists at $existing."
    return 0
  fi
  local stamp tmp dest f
  stamp=$(date +%Y%m%d-%H%M%S)
  dest=$library/backup-v5-$stamp
  # Staged under a hidden name, so a failed copy never passes for a finished backup.
  tmp=$library/.backup-v5-$stamp.partial
  mkdir "$tmp"
  for f in tf2tools.db tf2tools.db-wal tf2tools.db-shm; do
    if [ -f "$library/$f" ]; then
      cp -p "$library/$f" "$tmp/$f" || { rm -rf "$tmp"; die "could not copy $library/$f"; }
    fi
  done
  mv "$tmp" "$dest"
  echo "Backed up your TF2Tools v5 library database to $dest."
  echo "v6 upgrades the library on its first launch. v5 can still open it; the backup is a precaution."
  echo "To go back to v5, copy the files from that backup over $db."
}

if [ "$action" = install ]; then
  backup_v5_library
  bin=${TF2TOOLS_BIN:-}
  if [ -z "$bin" ]; then
    (cd "$ROOT" && CARGO_BUILD_JOBS=${JOBS:-4} "$cargo" build --release --locked -p tf2tools-app)
    bin=${CARGO_TARGET_DIR:-$ROOT/target}/release/tf2tools
    case $bin in /*) ;; *) bin=$ROOT/$bin ;; esac
  fi
  [ -x "$bin" ] || die "no binary at $bin"

  # The app finds its assets beside the binary it resolves to, so the link keeps them together.
  rm -rf "$libdir"
  mkdir -p "$libdir" "$prefix/bin"
  cp "$bin" "$libdir/tf2tools"
  chmod 755 "$libdir/tf2tools"
  cp -R "$ROOT/assets" "$libdir/assets"
  ln -sfn "$libdir/tf2tools" "$link"

  for pair in $icon_sizes; do
    mkdir -p "$data/icons/hicolor/${pair%%:*}/apps"
    cp -f "$ROOT/packaging/icons/${pair#*:}" "$data/icons/hicolor/${pair%%:*}/apps/tf2tools.png"
  done
  # Exec quoted per the desktop entry spec: backslash, quote, backtick and $ escaped, % doubled.
  exec_path=$(printf '%s' "$link" | sed -e 's/\\/\\\\\\\\/g' -e 's/["`$]/\\\\&/g' -e 's/%/%%/g')
  mkdir -p "$(dirname "$desktop")"
  cat > "$desktop" << EOF
[Desktop Entry]
Type=Application
Name=TF2Tools
Comment=Manage Team Fortress 2 HUDs, hitsounds, crosshairs and addons
Exec="$exec_path"
Icon=tf2tools
Terminal=false
Categories=Game;
StartupWMClass=com.tf2tools.app
EOF
  refresh_caches
  echo "Installed tf2tools in $prefix/bin, with a desktop entry at $desktop."
else
  rm -f "$link" "$desktop"
  rm -rf "$libdir"
  for pair in $icon_sizes; do
    rm -f "$data/icons/hicolor/${pair%%:*}/apps/tf2tools.png"
  done
  refresh_caches
  echo "Uninstalled tf2tools. Your library in $library is untouched."
fi
