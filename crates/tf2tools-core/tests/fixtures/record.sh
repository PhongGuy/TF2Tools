#!/usr/bin/env bash
# Records the store fixtures the store tests replay through wiremock. Run once by hand;
# tests never touch the network. Trims every page to a few records so fixtures stay small.
# Usage: record.sh [out-dir]   (default: the store/ directory next to this script)
set -euo pipefail
out="${1:-$(dirname "$0")/store}"
mkdir -p "$out" && out=$(cd "$out" && pwd)
ua='tf2tools-research (+https://github.com)'
gb='https://gamebanana.com/apiv11'
get() { sleep 0.3; curl -fsS -A "$ua" "$1"; }

# Keep an Index page's metadata, its first N records and their first three images.
trim() {
  jq --argjson n "$1" '._aRecords |= (.[:$n] | map(if ._aPreviewMedia._aImages
    then ._aPreviewMedia._aImages |= .[:3] else . end))'
}

idx="_nPerpage=15&_aFilters%5BGeneric_Game%5D=297&_sSort=Generic_MostLiked"
get "$gb/Sound/Index?_nPage=1&$idx" | trim 3 > "$out/gb-sounds-page1.json"
total=$(jq '._aMetadata._nRecordCount' "$out/gb-sounds-page1.json")
last=$(( (total + 14) / 15 ))
get "$gb/Sound/Index?_nPage=$last&$idx" | trim 2 > "$out/gb-sounds-last.json"
get "$gb/Mod/Index?_nPage=1&$idx&_aFilters%5BGeneric_Category%5D=1105" | trim 3 > "$out/gb-crosshairs-page1.json"
get "$gb/Mod/Index?_nPage=1&$idx&_aFilters%5BGeneric_Category%5D=1649" | trim 3 > "$out/gb-huds-page1.json"

sound_id=$(jq '._aRecords[0]._idRow' "$out/gb-sounds-page1.json")
mod_id=$(jq '._aRecords[0]._idRow' "$out/gb-crosshairs-page1.json")
get "$gb/Sound/$sound_id?_csvProperties=_idRow,_sName,_aFiles" > "$out/gb-sound-files.json"
get "$gb/Mod/$mod_id?_csvProperties=_idRow,_sName,_aFiles" > "$out/gb-crosshair-files.json"

# The latest mastercomfig release, cut to two addon VPKs and one non-addon asset.
get 'https://api.github.com/repos/mastercomfig/mastercomfig/releases/latest' \
  | jq '{tag_name, assets: ([.assets[] | select(.name|startswith("mastercomfig-addon-"))][:2]
          + [.assets[] | select(.name|startswith("mastercomfig-addon-")|not)][:1])
          | map({name, browser_download_url, size, download_count})}' > "$out/github-mastercomfig-latest.json"

# hud-db's archive, cut to its README and two HUD files under the real hud-db-main/ layout.
tmp=$(mktemp -d); trap 'rm -rf "$tmp"' EXIT
curl -fsSL -A "$ua" -o "$tmp/hud-db.zip" 'https://github.com/mastercomfig/hud-db/archive/refs/heads/main.zip'
(cd "$tmp" && files='hud-db-main/README.md hud-db-main/hud-data/rayshud.json hud-db-main/hud-data/budhud.json'
  unzip -q hud-db.zip $files && rm -f "$out/hud-db-main.zip" && zip -qX "$out/hud-db-main.zip" $files)

date -Is > "$out/RECORDED"
