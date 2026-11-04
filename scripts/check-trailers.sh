#!/usr/bin/env bash
# Fails when a commit since the previous tag carries a co-author or session trailer.
# Usage: check-trailers.sh [REV] (default HEAD). With no earlier tag, REV's whole history is checked.
set -euo pipefail

rev=${1:-HEAD}

# The previous tag is the nearest one strictly before REV; describe fails on a root commit or an
# untagged history, and both mean "check everything".
if prev=$(git describe --tags --abbrev=0 "$rev^" 2> /dev/null); then
  range="$prev..$rev"
  since=$prev
else
  range=$rev
  since="the first commit"
fi

count=$(git rev-list --count "$range")

# One log pass: \x01 lines carry the short sha, every other line is message text matched at its start.
git log --format='%x01%h%n%B' "$range" | awk '
  /^\x01/ { sha = substr($0, 2); next }
  tolower($0) ~ /^(co-authored-by|claude-session):/ { print "FAIL: " sha " " $0 > "/dev/stderr"; bad = 1 }
  END { exit bad }
'

echo "trailers OK: $count commit(s) since $since"
