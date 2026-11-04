#!/usr/bin/env bash
# Makes a prerelease deb sort before its stable release: `deb-prerelease.sh DEB`.
# Debian orders `6.0.0-alpha.0` after `6.0.0` but `6.0.0~alpha.0` before it, so every `-` in the
# control Version becomes `~`. The deb is repacked in place under the same name with gzip members
# and root ownership, the shape scripts/test-package.sh reads. A Version without `-` is untouched.
set -euo pipefail

die() { echo "FAIL: $*" >&2; exit 1; }

[ $# -eq 1 ] || die "usage: deb-prerelease.sh DEB"
deb=$1
name=${deb##*/}
[ -f "$deb" ] || die "$deb does not exist"
command -v dpkg-deb > /dev/null || die "needs dpkg-deb"

old=$(dpkg-deb -f "$deb" Version)
case "$old" in
  *-*) ;;
  *) echo "$name: Version $old unchanged"; exit 0 ;;
esac
# tr rather than ${old//-/~}, whose replacement bash may tilde-expand.
new=$(printf '%s' "$old" | tr -- - '~')

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
dpkg-deb -R "$deb" "$work/tree"
control=$work/tree/DEBIAN/control
awk -v v="$new" '/^Version:/ { print "Version: " v; next } { print }' "$control" > "$work/control"
cat "$work/control" > "$control"
dpkg-deb --root-owner-group -Zgzip -b "$work/tree" "$work/new.deb" > /dev/null
mv "$work/new.deb" "$deb"
echo "$name: Version $old -> $new"
