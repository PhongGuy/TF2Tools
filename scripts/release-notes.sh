#!/usr/bin/env bash
# Prints the release notes for TAG: TEMPLATE with {version} filled in and the {changes} line
# replaced by GENERATED, the body of GitHub's generated release notes (a file, or - for stdin).
# An optional INTRO file replaces the template's first <!-- ... --> line; a blank one changes nothing.
# Usage: release-notes.sh TAG TEMPLATE GENERATED [INTRO]
set -euo pipefail

[ $# -eq 3 ] || [ $# -eq 4 ] || { echo "usage: release-notes.sh TAG TEMPLATE GENERATED [INTRO]" >&2; exit 2; }
tag=$1
template=$2
generated=$3
intro_file=${4:-}
version=${tag#v}

[ -f "$template" ] || { echo "no template: $template" >&2; exit 1; }
[ "$generated" = - ] || [ -f "$generated" ] || { echo "no generated notes: $generated" >&2; exit 1; }
[ -z "$intro_file" ] || [ -f "$intro_file" ] || { echo "no intro: $intro_file" >&2; exit 1; }

# The template already carries the What's Changed heading, so GitHub's own copy is dropped along
# with the blank lines around the body. With no PR bullet left, players still get one line.
changes=$(mktemp)
intro=$(mktemp)
trap 'rm -f "$changes" "$intro"' EXIT
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

# The intro is the file without carriage returns or trailing blank lines; an empty result means
# there is no intro and the template keeps its comment.
if [ -n "$intro_file" ]; then
  tr -d '\r' < "$intro_file" | awk '
    { lines[++n] = $0 }
    END { while (n > 0 && lines[n] ~ /^[[:space:]]*$/) n--; for (i = 1; i <= n; i++) print lines[i] }
  ' > "$intro"
fi

# Plain string replacement, so a PR title holding & or \ comes through as written.
awk -v version="$version" -v changes="$changes" -v intro="$intro" '
  !swapped && /^<!--.*-->$/ && (getline first < intro) > 0 {
    swapped = 1; print first
    while ((getline line < intro) > 0) print line
    next
  }
  $0 == "{changes}" { while ((getline line < changes) > 0) print line; next }
  {
    out = ""
    while ((i = index($0, "{version}")) > 0) { out = out substr($0, 1, i - 1) version; $0 = substr($0, i + 9) }
    print out $0
  }
' "$template"
