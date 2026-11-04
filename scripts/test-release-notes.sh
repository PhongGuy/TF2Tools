#!/usr/bin/env bash
# Tests for scripts/release-notes.sh in a throwaway repository. Commits are made with plumbing
# (commit-tree), so no hook runs, and the user's git config is not read.
set -euo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
FAILS=0
fail() { echo "FAIL: $*" >&2; FAILS=$((FAILS + 1)); }

export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1
export GIT_AUTHOR_NAME=t GIT_AUTHOR_EMAIL=t@example.invalid GIT_COMMITTER_NAME=t GIT_COMMITTER_EMAIL=t@example.invalid
git init -q "$WORK/repo"
cd "$WORK/repo"
tree=$(git mktree < /dev/null)
commit() { # commit MESSAGE PARENT...; prints the new sha
  local message=$1 args=()
  shift
  for parent in "$@"; do args+=(-p "$parent"); done
  git commit-tree "$tree" "${args[@]}" -m "$message"
}

# A small template: the fixed text around the generated section, with the version in two places.
cat > "$WORK/template.md" << 'EOF'
<!-- intro -->

## What's Changed

{changes}

## Downloads

tf2tools_{version}_x86_64.AppImage and v{version}
EOF
render() { "$ROOT/scripts/release-notes.sh" "$1" "$WORK/template.md" > "$WORK/out" 2> "$WORK/err"; }
# The generated section: from the What's Changed heading up to the next heading.
changes() { awk '/^## What.s Changed$/ { on = 1; next } /^## / { on = 0 } on' "$WORK/out"; }

c1=$(commit "feat: the first release")
git tag -a v1.0.0 -m v1.0.0 "$c1"
c2=$(commit "feat(huds): show a preview for every HUD" "$c1")
c3=$(commit "fix(sounds): stop the player from hiding behind dialogs" "$c2")
c4=$(commit "$(printf 'chore: bump the version\n\nfeat: a body line is not a subject')" "$c3")
c5=$(commit "fix(ci): cache the build" "$c4")
c6=$(commit "feat(release): add checksums" "$c5")
c7=$(commit "fix(test): stabilise a flaky test" "$c6")
c8=$(commit "fix(deps): bump reqwest" "$c7")
c9=$(commit "docs: explain the store" "$c8")
side=$(commit "fix(crosshairs): keep the picked crosshair after a restart" "$c9")
c10=$(commit "feat: merge the side branch" "$c9" "$side")
c11=$(commit "feat!: Crosshairs & sounds share one \\ list" "$c10")
c12=$(commit "refactor(core): split the sync" "$c11")
git tag -a v1.1.0-beta.1 -m v1.1.0-beta.1 "$c12"

render v1.1.0-beta.1 || fail "a normal release: exit 1 [$(cat "$WORK/err")]"
want=$(cat << 'EOF'

- Show a preview for every HUD
- Stop the player from hiding behind dialogs
- Keep the picked crosshair after a restart
- Crosshairs & sounds share one \ list

**Full Changelog**: https://github.com/PhongGuy/TF2Tools/compare/v1.0.0...v1.1.0-beta.1

EOF
)
[ "$(changes)" = "$want" ] || fail "a normal release: What's Changed was [$(changes)]"
grep -qxF 'tf2tools_1.1.0-beta.1_x86_64.AppImage and v1.1.0-beta.1' "$WORK/out" \
  || fail "the version is not filled in: [$(cat "$WORK/out")]"
grep -qxF '<!-- intro -->' "$WORK/out" || fail "the intro slot is gone"
grep -qF '{' "$WORK/out" && fail "a placeholder is left: [$(cat "$WORK/out")]"

# Only commits since the previous v* tag count; a tag of another shape is not a release.
c13=$(commit "chore: tidy" "$c12")
git tag not-a-release "$c13"
c14=$(commit "docs: more words" "$c13")
git tag v1.1.0 "$c14"
render v1.1.0 || fail "nothing to list: exit 1"
want=$(printf '\n- Small fixes and improvements.\n\n**Full Changelog**: https://github.com/PhongGuy/TF2Tools/compare/v1.1.0-beta.1...v1.1.0\n')
[ "$(changes)" = "$want" ] || fail "nothing to list: What's Changed was [$(changes)]"

# The first release has no earlier tag, so it lists its whole history and links no comparison.
render v1.0.0 || fail "the first release: exit 1"
[ "$(changes)" = "$(printf '\n- The first release\n')" ] || fail "the first release: [$(changes)]"
grep -q 'Full Changelog' "$WORK/out" && fail "the first release links a comparison"

if render v9.9.9; then fail "an unknown tag: exit 0"; fi
if "$ROOT/scripts/release-notes.sh" v1.1.0 > /dev/null 2>&1; then fail "no template: exit 0"; fi

[ "$FAILS" -eq 0 ] || { echo "$FAILS release notes check(s) failed" >&2; exit 1; }
echo "release notes OK"
