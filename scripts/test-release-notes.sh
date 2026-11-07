#!/usr/bin/env bash
# Tests for scripts/release-notes.sh against canned bodies shaped like GitHub's generated release
# notes, so no network or repository is needed.
set -euo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
FAILS=0
fail() { echo "FAIL: $*" >&2; FAILS=$((FAILS + 1)); }

# A small template: the fixed text around the generated section, with the version in two places.
cat > "$WORK/template.md" << 'EOF'
<!-- intro -->

## What's Changed

{changes}

## Downloads

tf2tools_{version}_x86_64.AppImage and v{version}
EOF
render() { "$ROOT/scripts/release-notes.sh" "$1" "$WORK/template.md" "$2" > "$WORK/out" 2> "$WORK/err"; }
# The generated section: from the template's What's Changed heading up to Downloads.
changes() { awk '/^## What.s Changed$/ && !seen { seen = 1; on = 1; next } /^## Downloads$/ { on = 0 } on' "$WORK/out"; }

# A generate-notes body as GitHub returns it: a config comment, its own heading, PRs grouped by
# label, a New Contributors section and the comparison link, with CRLF line ends.
printf '%s\r\n' \
  '<!-- Release notes generated using configuration in .github/release.yml at v1.1.0 -->' \
  '' \
  "## What's Changed" \
  '### New features' \
  '* Show a preview for every HUD by @p in https://github.com/PhongGuy/TF2Tools/pull/12' \
  '### Fixes' \
  '* Crosshairs & sounds share one \ list by @p in https://github.com/PhongGuy/TF2Tools/pull/13' \
  '' \
  '## New Contributors' \
  '* @q made their first contribution in https://github.com/PhongGuy/TF2Tools/pull/13' \
  '' \
  '**Full Changelog**: https://github.com/PhongGuy/TF2Tools/compare/v1.0.0...v1.1.0' > "$WORK/generated.md"

render v1.1.0 "$WORK/generated.md" || fail "a normal release: exit 1 [$(cat "$WORK/err")]"
want=$(cat << 'EOF'

<!-- Release notes generated using configuration in .github/release.yml at v1.1.0 -->

### New features
* Show a preview for every HUD by @p in https://github.com/PhongGuy/TF2Tools/pull/12
### Fixes
* Crosshairs & sounds share one \ list by @p in https://github.com/PhongGuy/TF2Tools/pull/13

## New Contributors
* @q made their first contribution in https://github.com/PhongGuy/TF2Tools/pull/13

**Full Changelog**: https://github.com/PhongGuy/TF2Tools/compare/v1.0.0...v1.1.0

EOF
)
[ "$(changes)" = "$want" ] || fail "a normal release: What's Changed was [$(changes)]"
[ "$(grep -c "^## What.s Changed" "$WORK/out")" -eq 1 ] || fail "the heading is doubled: [$(cat "$WORK/out")]"
grep -qxF 'tf2tools_1.1.0_x86_64.AppImage and v1.1.0' "$WORK/out" \
  || fail "the version is not filled in: [$(cat "$WORK/out")]"
grep -qxF '<!-- intro -->' "$WORK/out" || fail "the intro slot is gone"
grep -q $'\r' "$WORK/out" && fail "a carriage return is left"
grep -qF '{' "$WORK/out" && fail "a placeholder is left: [$(cat "$WORK/out")]"

# No PR since the previous release: GitHub returns only the comparison link.
printf '%s\n' '**Full Changelog**: https://github.com/PhongGuy/TF2Tools/compare/v1.1.0...v1.1.1' > "$WORK/empty.md"
render v1.1.1 - < "$WORK/empty.md" || fail "nothing to list from stdin: exit 1 [$(cat "$WORK/err")]"
want=$(printf '\n- Small fixes and improvements.\n\n**Full Changelog**: https://github.com/PhongGuy/TF2Tools/compare/v1.1.0...v1.1.1\n')
[ "$(changes)" = "$want" ] || fail "nothing to list: What's Changed was [$(changes)]"

# An empty body still yields the fallback line.
: > "$WORK/blank.md"
render v1.0.0 "$WORK/blank.md" || fail "an empty body: exit 1"
[ "$(changes)" = "$(printf '\n- Small fixes and improvements.\n')" ] || fail "an empty body: [$(changes)]"

if render v1.1.0 "$WORK/missing.md"; then fail "a missing generated file: exit 0"; fi
if "$ROOT/scripts/release-notes.sh" v1.1.0 "$WORK/template.md" > /dev/null 2>&1; then fail "no generated notes argument: exit 0"; fi
if "$ROOT/scripts/release-notes.sh" v1.1.0 "$WORK/nope.md" "$WORK/generated.md" > /dev/null 2>&1; then fail "no template: exit 0"; fi

[ "$FAILS" -eq 0 ] || { echo "$FAILS release notes check(s) failed" >&2; exit 1; }
echo "release notes OK"
