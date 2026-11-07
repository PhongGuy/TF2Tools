#!/usr/bin/env bash
# Fails when a commit since the previous tag credits an AI agent, as judged by the same library as
# the PR attribution check; a human Co-authored-by passes.
# Usage: check-trailers.sh [REV] (default HEAD). With no earlier tag, REV's whole history is checked.
set -euo pipefail

rev=${1:-HEAD}
# shellcheck source=../.github/scripts/attribution-lib.sh
. "$(dirname "${BASH_SOURCE[0]}")/../.github/scripts/attribution-lib.sh"

# The previous tag is the nearest one strictly before REV; describe fails on a root commit or an
# untagged history, and both mean "check everything".
if prev=$(git describe --tags --abbrev=0 "$rev^" 2> /dev/null); then
  range="$prev..$rev"
  since=$prev
else
  range=$rev
  since="the first commit"
fi

# find_attribution writes the offending lines to stderr, captured here so they follow the FAIL line.
# It runs only as an if condition, where errexit is off, since its inner greps fail on clean text.
bad=0
count=0
for sha in $(git rev-list "$range"); do
  count=$((count + 1))
  if hits=$(git log -1 --format=%B "$sha" | find_attribution 2>&1); then
    echo "FAIL: $(git rev-parse --short "$sha") credits an AI agent" >&2
    printf '%s\n' "$hits" >&2
    bad=1
  fi
done
[ "$bad" -eq 0 ] || exit 1

echo "trailers OK: $count commit(s) since $since"
