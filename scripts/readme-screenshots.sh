#!/usr/bin/env bash
# Regenerates the README screenshots in .github/readme: `make readme-shots`. Drives the real app
# (TF2TOOLS_PILOT_REAL=1: your own library, TF2 install and the online stores) through fourteen
# pages with gpui-pilot's drive.py and saves each page as lossless WebP. Besides looking, it
# installs your first library HUD if none is installed, and opens the sidebar; it adds a
# "Competitive" profile for the Profiles shot and deletes it, and closes the sidebar again if it
# was closed. README_SHOTS_DRY_RUN=1 prints the drive calls instead, with stand-in answers;
# README_SHOTS_CHECK=<dir> only checks a shots dir. Inputs: CARGO, CARGO_TARGET_DIR,
# GPUI_PILOT, GPUI_PILOT_REPO. Needs magick. Must stay bash 3.2 safe (macOS ships 3.2).
set -euo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cargo=${CARGO:-cargo}
pilot_repo=${GPUI_PILOT_REPO:-${HOME:?HOME is not set}/git/gpui-pilot}
NAMES="huds sounds crosshairs hud-store sound-store crosshair-store addon-store profiles settings graphics maps logs-tf hud-maker crosshair-maker"
COUNT=14
# Shots cut to a box (WxH+X+Y): the Sound Store loses the bottom strip where the floating
# volume bar would cover a card.
crop_of() {
  case $1 in
    sound-store) echo 1280x736+0+0 ;;
    *) echo "" ;;
  esac
}

# Stops the run. Mid-drive it first puts back what the run changed and closes the app.
# Labels the conversion paints over with what is just above them: the installed HUD's folder
# path, which holds the player's home path, and the logs.tf player chips, which name the player.
# An entry ending in ":" covers every id it starts, painted as one box.
BLANKED="huds-installed-path logstf-chip: logstf-chip-rename: logstf-chip-remove: logstf-add"

die() {
  echo "readme-shots: $*" >&2
  if [ -n "${drive:-}" ] && [ -z "${dying:-}" ]; then
    dying=1
    undo
    send close
    exec 3>&-
    wait "$drive" || true
    cat "$S/drive.log" >&2
  fi
  exit 1
}

# Fails unless shots dir $1 holds every screenshot and screen trees in which no on-screen label
# shows a home path or the HUD Maker shows the player's own backdrop picture.
check_shots() {
  local dir=$1
  set -- "$dir"/tree-*.json
  [ -f "$1" ] || die "no screen trees in $dir to check for home paths"
  local why
  why=$(python3 - "${HOME:?HOME is not set}" "$BLANKED" "$@" << 'EOF'
import json, sys
home, blanked, trees = sys.argv[1], sys.argv[2].split(), sys.argv[3:]
for path in trees:
    try:
        nodes = json.load(open(path))["nodes"]
    except (OSError, ValueError, KeyError, TypeError):
        print(f"could not read the screen tree {path}")
        sys.exit(1)
    for node in nodes:
        if not node.get("visible", True):
            continue
        ids = [str(seg) for seg in node.get("path", [])]
        if ids and any(ids[-1] == b or (b.endswith(":") and ids[-1].startswith(b)) for b in blanked):
            continue
        if ids and ids[-1] == "hud-maker-backdrop-image":
            print("a screenshot shows the HUD Maker backdrop picture")
            sys.exit(1)
        text = " ".join(str(node.get(k) or "") for k in ("name", "value"))
        if home in text or "/home/" in text or "/Users/" in text:
            print("a screenshot shows a home path")
            sys.exit(1)
EOF
  ) || die "${why:-could not check the screen trees in $dir}"
  set -- "$dir"/shot-*.png
  [ $# -eq $COUNT ] && [ -f "$dir/shot-$COUNT.png" ] || die "expected $COUNT screenshots, found $#"
}

# One drive call to drive.py, or to stdout in a dry run.
send() {
  if [ -n "$dry" ]; then printf '%s\n' "$1"; else printf '%s\n' "$1" >&3; fi
}

# Reads the screen now and sets ANSWER to question $1 about it ("first <id prefix>", "count
# <id prefix>" or "width <id>"); a dry run answers $2. Waits at most five minutes for the tree.
trees=0
ask() {
  if [ -n "$dry" ]; then
    send tree
    ANSWER=$2
    return 0
  fi
  trees=$((trees + 1))
  send tree
  local tree=$S/shots/tree-$trees.json n=0
  until grep -qF "saved to $tree" "$S/drive.log"; do
    kill -0 "$drive" 2> /dev/null || die "drive.py stopped; see its log above"
    n=$((n + 1))
    [ $n -lt 300 ] || die "no screen tree after five minutes"
    sleep 1
  done
  ANSWER=$(python3 - "$tree" $1 << 'EOF'
import json, sys
path, what, arg = sys.argv[1], sys.argv[2], sys.argv[3]
nodes = [n for n in json.load(open(path))["nodes"] if n.get("visible", True)]
ids = [(str(n["path"][-1]) if n.get("path") else "", n) for n in nodes]
if what == "first":
    print(next((i for i, _ in ids if i.startswith(arg)), ""))
elif what == "count":
    print(sum(1 for i, _ in ids if i.startswith(arg)))
elif what == "width":
    print(next((int(n["bounds"]["w"]) for i, n in ids if i == arg), 0))
EOF
  ) || die "could not read the screen tree $tree"
}

# Moves the pointer onto the sidebar's empty middle first, so no tooltip or hover outline shows.
shot() {
  send "# shot $1"
  send 'hover {"target": {"x": 24, "y": 690}}'
  send 'wait {"ms": 1500}'
  send screenshot
  send tree
  [ -n "$dry" ] || { trees=$((trees + 1)); echo "$trees" >> "$S/shot-trees"; }
}

click() { send "click {\"target\": {\"id_suffix\": \"$1\"}}"; }

# The installed HUD's missing fonts are skipped, never installed onto the machine; the app asks
# at start and after an install.
skip_fonts() {
  ask "count hud-fonts-skip" 0
  [ "$ANSWER" = 0 ] || click hud-fonts-skip
}

# Waits, ten seconds at a time for at most $2 minutes, until no id starts with $1.
wait_gone() {
  local n=0
  while ask "count $1" 0 && [ "$ANSWER" != 0 ]; do
    n=$((n + 1))
    [ $n -lt $(($2 * 6)) ] || die "$1 still shows after $2 minutes; try again later"
    send 'wait {"ms": 10000}'
  done
}

# Waits at most thirty minutes until store $1's sync ring is gone; a full resync takes that long.
wait_sync() { wait_gone "sync-ring:$1" 30; }

# What the run changed and still has to undo: the sidebar it opened, the profile it added.
opened_sidebar=""
profile=""
undo() {
  [ -n "$profile" ] && { click nav-profiles; click "$profile"; click confirm-ok; profile=""; }
  [ -n "$opened_sidebar" ] && { click sidebar-toggle; opened_sidebar=""; }
  return 0
}

# Every drive call in order. The Crosshairs page only searches and selects (a weapon row click
# would assign a crosshair), the Crosshair Maker's starter is never saved, and the HUD Maker
# shows the main menu, which draws the game's own menu picture instead of the player's backdrop.
calls() {
  local page
  send "launch {\"mode\": \"headless\", \"build\": false, \"width\": 1280, \"height\": 800, \"scale\": 1, \"env\": {\"TF2TOOLS_PILOT_REAL\": \"1\", \"TF2TOOLS_ASSETS\": \"$ROOT/assets\", \"TF2TOOLS_PILOT_REDUCE_MOTION\": \"1\"}}"
  send 'wait {"ms": 3000}'
  skip_fonts
  click nav-huds
  # New players see the sidebar with its labels; a closed one is about 48 px wide.
  ask "width sidebar" 48
  if [ "$ANSWER" -lt 100 ]; then
    click sidebar-toggle
    opened_sidebar=1
  fi
  ask "count huds-installed-empty" 1
  if [ "$ANSWER" != 0 ]; then
    ask "first hud-install:" hud-install:example-hud
    [ -n "$ANSWER" ] || die "the library has no HUD to install"
    click "$ANSWER"
    skip_fonts
    send 'wait {"target": {"id_suffix": "huds-installed-empty"}, "gone": true, "timeout_ms": 60000}'
    send 'wait {"ms": 5000}'
  fi
  shot huds
  click nav-sounds
  shot sounds
  # ql 3 is one of the crosshairs bundled with the app, so every library has it.
  click nav-crosshairs
  send 'type {"target": {"id_suffix": "crosshairs-search"}, "text": "ql 3"}'
  click crosshair-option:builtin:ql_3
  shot crosshairs
  for page in hud-store sound-store crosshair-store addon-store; do
    click "nav-$page"
    send 'wait {"ms": 3000}'
    [ "$page" != sound-store ] || wait_sync sounds
    [ "$page" != crosshair-store ] || wait_sync crosshairs
    shot "$page"
  done
  # A neutral profile, so no name from the player's own list shows; refused if one exists.
  click nav-profiles
  send 'type {"target": {"id_suffix": "profiles-search"}, "text": "Competitive"}'
  ask "count profile-delete:" 0
  [ "$ANSWER" = 0 ] || die "a profile matching Competitive exists; rename it first"
  # New profile selects its "Profile N" name, so an untargeted type replaces it; Back saves it.
  click profiles-new
  # The profile is created off the UI thread; typing before the editor opens would hit the search.
  send 'wait {"target": {"id_suffix": "profile-editor-back"}, "timeout_ms": 20000}'
  send 'type {"text": "Competitive"}'
  click profile-editor-back
  send 'wait {"ms": 2000}'
  ask "first profile-delete:" profile-delete:example-profile
  profile=$ANSWER
  [ -n "$profile" ] || die "the Competitive profile was not saved"
  shot profiles
  click "$profile"
  click confirm-ok
  profile=""
  # The top two cards show the pilot's own version and the home path of the TF2 folder.
  click nav-settings
  send 'scroll {"target": {"id_suffix": "page-settings"}, "dy": 470}'
  shot settings
  click nav-graphics
  send 'wait {"target": {"id_suffix": "graphics-status"}, "timeout_ms": 20000}'
  send 'wait {"ms": 2000}'
  shot graphics
  # The GameBanana tab once its catalogue sync is done; with auto-sync off it syncs now.
  click nav-maps
  click maps-source:gamebanana
  send 'wait {"ms": 3000}'
  ask "count maps-gb-sync" 0
  [ "$ANSWER" = 0 ] || { click maps-gb-sync; send 'wait {"ms": 3000}'; }
  wait_gone sync-banner 20
  ask "count addon-gb-card-image:maps:" 1
  [ "$ANSWER" != 0 ] || die "the GameBanana maps tab shows no map"
  send 'wait {"ms": 8000}'
  shot maps
  # The tracked player's most played class with its weapons open; their chips are painted
  # over (BLANKED), and no log opens, since an open log names everyone in it.
  click nav-logs-tf
  send 'wait {"ms": 3000}'
  ask "count logstf-chip:" 1
  [ "$ANSWER" != 0 ] || die "no logs.tf player is tracked"
  wait_gone sync-ring:logs-tf 10
  click logstf-class-head:Played
  ask "first logstf-class:" logstf-class:soldier
  click "$ANSWER"
  send 'wait {"target": {"id_suffix": "logstf-class-weapons"}, "timeout_ms": 30000}'
  send 'wait {"ms": 2000}'
  shot logs-tf
  click nav-hud-maker
  send 'click {"target": {"id_suffix": "hud-maker-scene:main-menu"}, "timeout_ms": 30000}'
  send 'wait {"target": {"id_suffix": "hud-maker-menu-background"}, "timeout_ms": 20000}'
  click hud-maker-part:MainMenuOverride/CharacterSetupButton
  shot hud-maker
  # Last, since leaving an unsaved drawing asks before it is dropped.
  click nav-crosshair-maker
  click crosshair-maker-starters
  click crosshair-maker-starter:circle-dot
  click crosshair-maker-starter-use
  shot crosshair-maker
  undo
  send close
}

if [ -n "${README_SHOTS_CHECK:-}" ]; then
  check_shots "$README_SHOTS_CHECK"
  exit 0
fi

dry=${README_SHOTS_DRY_RUN:-}
if [ -n "$dry" ]; then
  calls
  exit 0
fi

command -v magick > /dev/null || die "needs ImageMagick's magick"
[ -f "$pilot_repo/scripts/drive.py" ] || die "no drive.py under $pilot_repo/scripts"
cd "$ROOT"

# The shots land here before conversion; mkdir refuses a dir this run did not make, so the
# trap only ever removes our own.
S=${TMPDIR:-/tmp}/tf2tools-readme-shots
mkdir "$S" 2> /dev/null || die "$S already exists; remove it if a previous run left it"
trap 'rm -rf "$S"' EXIT
# Turn signals into a plain exit so the EXIT cleanup runs on every bash, 3.2 included.
trap 'exit 130' INT
trap 'exit 143' TERM
trap 'exit 129' HUP

"$cargo" build --manifest-path tools/tf2tools-pilot/Cargo.toml --bin tf2tools-pilot
built=${CARGO_TARGET_DIR:-tools/tf2tools-pilot/target}/debug
# gpui-pilot.toml names this path for the headless binary; point it at a build made elsewhere.
expected=tools/tf2tools-pilot/target/debug/tf2tools-pilot
if [ "$(cd "$(dirname "$built/tf2tools-pilot")" && pwd)/tf2tools-pilot" != "$ROOT/$expected" ]; then
  mkdir -p "$(dirname "$expected")"
  ln -sfn "$(cd "$built" && pwd)/tf2tools-pilot" "$expected"
fi

# Calls go through a FIFO so `ask` can read a screen tree mid-run and pick the next call.
mkfifo "$S/calls"
python3 -u "$pilot_repo/scripts/drive.py" --out "$S/shots" < "$S/calls" > "$S/drive.log" 2>&1 &
drive=$!
exec 3> "$S/calls"
calls
exec 3>&-
rc=0
wait "$drive" || rc=$?
cat "$S/drive.log"
[ $rc -eq 0 ] || die "a drive call failed; see the log above"

check_shots "$S/shots"

mkdir -p .github/readme
i=0
total=0
for name in $NAMES; do
  i=$((i + 1))
  out=.github/readme/$name.webp
  png=$S/shots/shot-$i.png
  set --
  # Each blanked label's box, filled with the card colour just above it.
  for box in $(python3 - "$S/shots/tree-$(sed -n "${i}p" "$S/shot-trees").json" "$BLANKED" << 'EOF'
import json, sys
nodes, blanked = json.load(open(sys.argv[1]))["nodes"], sys.argv[2].split()
for entry in blanked:
    boxes = []
    for n in nodes:
        last = str(n["path"][-1]) if n.get("path") else ""
        hit = last.startswith(entry) if entry.endswith(":") else last == entry
        if hit and n.get("visible", True):
            b = n["bounds"]
            x, y, w, h = int(b["x"]), int(b["y"]), int(b["w"]), int(b["h"])
            boxes.append((x, y, x + w, y + h))
    if boxes:
        x0, y0 = min(b[0] for b in boxes), min(b[1] for b in boxes)
        x1, y1 = max(b[2] for b in boxes), max(b[3] for b in boxes)
        print(f"{x0},{y0},{x1},{y1}")
EOF
  ); do
    IFS=, read -r x0 y0 x1 y1 << EOF
$box
EOF
    fill=$(magick "$png" -format "%[pixel:p{$((x0 + 4)),$((y0 - 3))}]" info:)
    set -- "$@" -fill "$fill" -draw "rectangle $x0,$y0 $x1,$y1"
  done
  crop=$(crop_of "$name")
  [ -z "$crop" ] || set -- "$@" -crop "$crop" +repage
  magick "$png" "$@" -strip -define webp:lossless=true -define webp:method=6 "$out"
  bytes=$(wc -c < "$out" | tr -d ' ')
  total=$((total + bytes))
  echo "$out $bytes"
done
echo "total $total"
