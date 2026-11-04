#!/usr/bin/env bash
# Layout tests over the packages `make package` builds: test-package.sh DIR, where DIR holds
# exactly one tf2tools_*.deb and one tf2tools_*.AppImage. Reads the packages, never installs.
set -euo pipefail

dir=${1:?usage: test-package.sh DIR}
[ -d "$dir" ] || { echo "FAIL: no directory $dir" >&2; exit 1; }
# Absolute, since the deb checks run from a temp dir.
dir=$(cd "$dir" && pwd)
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
FAILS=0
fail() { echo "FAIL: $*" >&2; FAILS=$((FAILS + 1)); }
# Exits 1 with the failure count when any check failed.
stop_on_fails() {
  [ "$FAILS" -eq 0 ] || { echo "$FAILS package check(s) failed" >&2; exit 1; }
}

one() {
  local matches=("$dir"/$1)
  [ ${#matches[@]} -eq 1 ] && [ -f "${matches[0]}" ] || { echo "FAIL: want one $1 in $dir" >&2; exit 1; }
  printf '%s\n' "${matches[0]}"
}
deb=$(one 'tf2tools_*_amd64.deb')
appimage=$(one 'tf2tools_*_x86_64.AppImage')

# The deb: an ar archive of control.tar.gz and data.tar.gz (cargo-packager 0.11.8).
mkdir "$WORK/deb"
(cd "$WORK/deb" && ar x "$deb")
listing=$(tar tvzf "$WORK/deb/data.tar.gz" | awk '{ print $1, $NF }' | sed 's# \./# #')
# A here-string, not a pipe: under pipefail an early `grep -q` exit fails the writer with SIGPIPE.
has() { grep -q "$1" <<< "$listing"; }
has '^-rwxr-xr-x usr/bin/tf2tools$' || fail "deb: usr/bin/tf2tools missing or not 0755"
has ' usr/lib/tf2tools/assets/crosshairs/crosshairs.json$' || fail "deb: crosshair manifest missing"
for size in 32x32 128x128 512x512; do
  has " usr/share/icons/hicolor/$size/apps/tf2tools.png$" || fail "deb: $size icon missing"
done
entries=$(printf '%s\n' "$listing" | awk '{ print $2 }' | grep '^usr/share/applications/.*\.desktop$' || true)
[ "$entries" = usr/share/applications/com.tf2tools.app.desktop ] || fail "deb: desktop entries [$entries]"
mkdir "$WORK/deb/data"
tar xzf "$WORK/deb/data.tar.gz" -C "$WORK/deb/data"
grep -qx 'StartupWMClass=com.tf2tools.app' "$WORK/deb/data/usr/share/applications/com.tf2tools.app.desktop" \
  || fail "deb: desktop entry lacks StartupWMClass=com.tf2tools.app"
control=$(tar xOzf "$WORK/deb/control.tar.gz" control)
grep -qx 'Package: tf2tools' <<< "$control" || fail "deb: Package is not tf2tools"
grep -q '^Depends: .*libvulkan1' <<< "$control" || fail "deb: Depends lacks libvulkan1"
# ALSA's 24.04 name first, or apt satisfies the virtual libasound2 there with the OSS shim.
grep -Eq '^Depends: libasound2t64 \| libasound2(,|$)' <<< "$control" \
  || fail "deb: Depends does not start with libasound2t64 | libasound2"

# The AppImage: extracted with its own runtime, which needs no FUSE for --appimage-extract.
# A corrupt or non-executable AppImage ends the run here, since every later check reads its tree.
if ! (cd "$WORK" && "$appimage" --appimage-extract > /dev/null 2>&1); then
  fail "appimage: $(basename "$appimage") is not executable or does not extract"
  stop_on_fails
fi
root=$WORK/squashfs-root
[ -x "$root/AppRun" ] || fail "appimage: AppRun missing"
[ -x "$root/usr/bin/tf2tools" ] || fail "appimage: usr/bin/tf2tools missing or not executable"
[ -f "$root/usr/lib/tf2tools/assets/crosshairs/crosshairs.json" ] || fail "appimage: crosshair manifest missing"
desktops=$(cd "$root" && command ls -1 ./*.desktop 2> /dev/null | tr '\n' ' ')
[ "$desktops" = "./tf2tools.desktop " ] || fail "appimage: root desktop entries [$desktops]"
case $(readlink "$root/tf2tools.desktop" || true) in
  usr/share/applications/tf2tools.desktop) ;;
  *) fail "appimage: tf2tools.desktop does not link into usr/share/applications" ;;
esac
grep -qx 'StartupWMClass=com.tf2tools.app' "$root/tf2tools.desktop" || fail "appimage: StartupWMClass"
grep -qx 'Icon=tf2tools' "$root/tf2tools.desktop" || fail "appimage: Icon"
bundled=$(find "$root/usr" -name 'libvulkan*' -o -name 'libGL*' -o -name 'libEGL*' | tr '\n' ' ')
[ -z "$bundled" ] || fail "appimage: bundles graphics libraries [$bundled]"

stop_on_fails
echo "package layout OK: $(basename "$deb"), $(basename "$appimage")"
