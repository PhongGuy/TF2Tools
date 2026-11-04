#!/usr/bin/env bash
# Prints the release notes for TAG: TEMPLATE with {version} filled in and the {changes} line
# replaced by the player-facing feat and fix commits since the previous v* tag, oldest first.
# Usage: release-notes.sh TAG TEMPLATE
set -euo pipefail

[ $# -eq 2 ] || { echo "usage: release-notes.sh TAG TEMPLATE" >&2; exit 2; }
tag=$1
template=$2
version=${tag#v}
repo_url=https://github.com/PhongGuy/TF2Tools

git rev-parse -q --verify "$tag^{commit}" > /dev/null || { echo "no such tag: $tag" >&2; exit 1; }
[ -f "$template" ] || { echo "no template: $template" >&2; exit 1; }

# The previous release is the nearest v* tag strictly before TAG; none means the first release.
if prev=$(git describe --tags --abbrev=0 --match 'v*' "$tag^" 2> /dev/null); then
  range="$prev..$tag"
else
  prev=
  range=$tag
fi

# Only feat and fix subjects reach players; build, release, test and dependency work does not.
changes=$(mktemp)
trap 'rm -f "$changes"' EXIT
pattern='^(feat|fix)(\(([^)]*)\))?!?: (.+)$'
while IFS= read -r subject; do
  [[ $subject =~ $pattern ]] || continue
  case ${BASH_REMATCH[3]} in ci | release | test | deps) continue ;; esac
  text=${BASH_REMATCH[4]}
  printf -- '- %s\n' "${text^}" >> "$changes"
done < <(git log --no-merges --reverse --format=%s "$range")
[ -s "$changes" ] || echo "- Small fixes and improvements." > "$changes"
if [ -n "$prev" ]; then
  printf '\n**Full Changelog**: %s/compare/%s...%s\n' "$repo_url" "$prev" "$tag" >> "$changes"
fi

# Plain string replacement, so a subject holding & or \ comes through as written.
awk -v version="$version" -v changes="$changes" '
  $0 == "{changes}" { while ((getline line < changes) > 0) print line; next }
  {
    out = ""
    while ((i = index($0, "{version}")) > 0) { out = out substr($0, 1, i - 1) version; $0 = substr($0, i + 9) }
    print out $0
  }
' "$template"
