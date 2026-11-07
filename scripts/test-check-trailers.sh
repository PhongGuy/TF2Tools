#!/usr/bin/env bash
# Tests for scripts/check-trailers.sh in a throwaway repository. Commits are made with plumbing
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
commit() { # commit PARENT MESSAGE; prints the new sha
  if [ -n "$1" ]; then git commit-tree "$tree" -p "$1" -m "$2"; else git commit-tree "$tree" -m "$2"; fi
}
check() { "$ROOT/scripts/check-trailers.sh" "$1" > "$WORK/out" 2> "$WORK/err"; }

# Each AI credit gets its own commit on c3, so every case is judged alone.
c1=$(commit "" "$(printf 'feat: one\n\nCo-Authored-By: Claude <noreply@example.invalid>')")
c2=$(commit "$c1" "fix: two")
git tag v1 "$c2"
c3=$(commit "$c2" "feat: three")
c4=$(commit "$c3" "$(printf 'feat: four\n\nClaude-Session: abc123')")
c5=$(commit "$c3" "$(printf 'docs: five\n\nThe rule bans a Co-authored-by: line naming Claude in any commit.')")
c6=$(commit "$c3" "$(printf 'fix: six\n\nco-authored-by: claude <c@example.invalid>')")
human=$(commit "$c3" "$(printf 'feat: pair work\n\nCo-authored-by: Jane Doe <123+jane@users.noreply.github.com>')")
scope=$(commit "$c3" "chore(claude): tidy the agent settings")
bot=$(commit "$c3" "$(printf 'fix: bot\n\nCo-authored-by: helper[bot] <1+helper[bot]@users.noreply.github.com>')")
footer=$(commit "$c3" "$(printf 'fix: footer\n\nGenerated with Claude Code')")
noreply=$(commit "$c3" "$(printf 'fix: address\n\nCo-authored-by: Someone <noreply@anthropic.com>')")
robot=$(commit "$c3" "$(printf 'fix: robot\n\n🤖 made this')")
nameless=$(commit "$c3" "$(printf 'fix: nameless\n\nCo-authored-by: someone')")

check "$c3" || fail "clean range since v1: exit 1 [$(cat "$WORK/err")]"
grep -q '^trailers OK: 1 commit(s) since v1$' "$WORK/out" || fail "clean range: [$(cat "$WORK/out")]"
if check "$c4"; then fail "Claude-Session: exit 0"; fi
grep -q "^FAIL: $(git rev-parse --short "$c4") credits an AI agent$" "$WORK/err" \
  && grep -q '^    Claude-Session: abc123$' "$WORK/err" || fail "Claude-Session: [$(cat "$WORK/err")]"
check "$c5" || fail "a mention inside a sentence: exit 1"
if check "$c6"; then fail "lowercase AI co-author: exit 0"; fi
check "$human" || fail "a human co-author: exit 1 [$(cat "$WORK/err")]"
check "$scope" || fail "claude as a commit scope: exit 1 [$(cat "$WORK/err")]"
for case in bot footer noreply robot nameless; do
  if check "${!case}"; then fail "$case: exit 0"; fi
done
# The tag itself: no tag before v1, so the whole history counts, including c1.
if check v1; then fail "whole history with c1's trailer: exit 0"; fi
grep -q "^FAIL: $(git rev-parse --short "$c1") credits an AI agent$" "$WORK/err" || fail "whole history: [$(cat "$WORK/err")]"
check "$c2" > /dev/null 2>&1 && fail "the root commit's trailer is missed from c2"
git tag v0 "$c1"
check "$c2" || fail "range v0..c2 excludes c1: exit 1"

[ "$FAILS" -eq 0 ] || { echo "$FAILS trailer check(s) failed" >&2; exit 1; }
echo "trailer check OK"
