#!/usr/bin/env bash
# Prints the release notes for TAG: TEMPLATE with {version} filled in and the {changes} line
# replaced by GENERATED, the body of GitHub's generated release notes (a file, or - for stdin).
# Usage: release-notes.sh TAG TEMPLATE GENERATED
set -euo pipefail

[ $# -eq 3 ] || { echo "usage: release-notes.sh TAG TEMPLATE GENERATED" >&2; exit 2; }
tag=$1
template=$2
generated=$3
version=${tag#v}

[ -f "$template" ] || { echo "no template: $template" >&2; exit 1; }
[ "$generated" = - ] || [ -f "$generated" ] || { echo "no generated notes: $generated" >&2; exit 1; }

# The template already carries the What's Changed heading, so GitHub's own copy is dropped along
# with the blank lines around the body. With no PR bullet left, players still get one line.
changes=$(mktemp)
trap 'rm -f "$changes"' EXIT
cat -- "$generated" | tr -d '\r' | awk '
  !dropped && /^## What.s Changed[[:space:]]*$/ { dropped = 1; next }
  { lines[++n] = $0 }
  /^[*-] / { bullets = 1 }
  END {
    first = 1; while (first <= n && lines[first] ~ /^[[:space:]]*$/) first++
    last = n; while (last >= first && lines[last] ~ /^[[:space:]]*$/) last--
    if (!bullets) { print "- Small fixes and improvements."; if (first <= last) print "" }
    for (i = first; i <= last; i++) print lines[i]
  }
' > "$changes"

# Plain string replacement, so a PR title holding & or \ comes through as written.
awk -v version="$version" -v changes="$changes" '
  $0 == "{changes}" { while ((getline line < changes) > 0) print line; next }
  {
    out = ""
    while ((i = index($0, "{version}")) > 0) { out = out substr($0, 1, i - 1) version; $0 = substr($0, i + 9) }
    print out $0
  }
' "$template"
