#!/usr/bin/env bash
# Tests for scripts/deb-prerelease.sh over small debs built here. Needs dpkg-deb; a host without
# it re-runs the suite in an ubuntu:22.04 container with no network.
set -euo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
if ! command -v dpkg-deb > /dev/null; then
  engine=$(command -v podman || command -v docker || true)
  [ -n "$engine" ] || { echo "FAIL: needs dpkg-deb, podman or docker" >&2; exit 1; }
  exec "$engine" run --rm --network none -v "$ROOT":/src:ro ubuntu:22.04 bash /src/scripts/test-deb-prerelease.sh
fi
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
FAILS=0
fail() { echo "FAIL: $*" >&2; FAILS=$((FAILS + 1)); }

# A deb shaped like cargo-packager's: gzip members, root-owned, a 0755 binary.
build() {
  local tree=$WORK/tree-$1
  mkdir -p "$tree/DEBIAN" "$tree/usr/bin"
  printf '#!/bin/sh\necho tf2tools\n' > "$tree/usr/bin/tf2tools"
  chmod 0755 "$tree/usr/bin/tf2tools"
  printf 'Package: tf2tools\nVersion: %s\nArchitecture: amd64\nMaintainer: TF2Tools contributors\nDescription: test\n' \
    "$1" > "$tree/DEBIAN/control"
  dpkg-deb --root-owner-group -Zgzip -b "$tree" "$2" > /dev/null
}

# The ar member names, read from the headers (the base image has no binutils).
members() {
  local off=8 size end
  end=$(stat -c %s "$1")
  while [ "$off" -lt "$end" ]; do
    printf '%s ' "$(dd if="$1" bs=1 skip="$off" count=16 status=none | tr -d ' ' | sed 's#/$##')"
    size=$(dd if="$1" bs=1 skip=$((off + 48)) count=10 status=none | tr -d ' ')
    off=$((off + 60 + size + size % 2))
  done
}

listing() { dpkg-deb -c "$1" | awk '{ print $1, $2, $NF }'; }

pre=$WORK/tf2tools_6.0.0-alpha.0_amd64.deb
build 6.0.0-alpha.0 "$pre"
before=$(listing "$pre")
dpkg --compare-versions 6.0.0-alpha.0 gt 6.0.0 || fail "premise: the hyphen form no longer sorts after 6.0.0"
"$ROOT/scripts/deb-prerelease.sh" "$pre" > "$WORK/out" || fail "prerelease: script failed"
[ -f "$pre" ] || fail "prerelease: the file name changed"
version=$(dpkg-deb -f "$pre" Version)
[ "$version" = 6.0.0~alpha.0 ] || fail "prerelease: Version is $version"
dpkg --compare-versions "$version" lt 6.0.0 || fail "prerelease: $version does not sort before 6.0.0"
dpkg --compare-versions "$version" gt 5.9.9 || fail "prerelease: $version sorts before 5.9.9"
grep -qx 'tf2tools_6.0.0-alpha.0_amd64.deb: Version 6.0.0-alpha.0 -> 6.0.0~alpha.0' "$WORK/out" \
  || fail "prerelease: summary was [$(cat "$WORK/out")]"
[ "$(listing "$pre")" = "$before" ] || fail "prerelease: contents, modes or owners changed"
[ "$(members "$pre")" = "debian-binary control.tar.gz data.tar.gz " ] \
  || fail "prerelease: members are [$(members "$pre")]"
[ "$(dpkg-deb -f "$pre" Package)" = tf2tools ] || fail "prerelease: other control fields lost"

stable=$WORK/tf2tools_6.0.0_amd64.deb
build 6.0.0 "$stable"
sum=$(sha256sum < "$stable")
"$ROOT/scripts/deb-prerelease.sh" "$stable" > "$WORK/out" || fail "stable: script failed"
[ "$(sha256sum < "$stable")" = "$sum" ] || fail "stable: the file changed"
grep -qx 'tf2tools_6.0.0_amd64.deb: Version 6.0.0 unchanged' "$WORK/out" || fail "stable: summary"

if "$ROOT/scripts/deb-prerelease.sh" "$WORK/missing.deb" 2> /dev/null; then
  fail "missing file: exit 0"
fi

[ "$FAILS" -eq 0 ] || { echo "$FAILS deb check(s) failed" >&2; exit 1; }
echo "deb prerelease OK"
