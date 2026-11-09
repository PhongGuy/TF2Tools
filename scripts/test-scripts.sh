#!/usr/bin/env bash
# Tests for scripts/install.sh, scripts/demo.sh, the Makefile's default goal and the package
# check's failure paths. Every run uses a prebuilt stub binary and temporary HOME, PREFIX and XDG
# dirs, so nothing builds and nothing outside this suite's own temp dir is read or written.
set -euo pipefail

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
FAILS=0
fail() { echo "FAIL: $*" >&2; FAILS=$((FAILS + 1)); }

export HOME=$WORK/home
mkdir -p "$HOME" "$WORK/bin"
unset XDG_DATA_HOME XDG_CONFIG_HOME XDG_CACHE_HOME TF2TOOLS_PREFIX DEMO_KEEP DEMO_DRY_RUN
# A process name nothing on this machine runs, so a real TF2Tools never changes a result.
export TF2TOOLS_V5_PROCESS=t2none$$
printf '#!/bin/sh\necho stub\n' > "$WORK/bin/tf2tools"
chmod +x "$WORK/bin/tf2tools"
export TF2TOOLS_BIN=$WORK/bin/tf2tools
# The launcher cache refreshers become no-ops, so they write no caches into the temp dirs.
for tool in update-desktop-database gtk-update-icon-cache; do
  printf '#!/bin/sh\nexit 0\n' > "$WORK/bin/$tool"
  chmod +x "$WORK/bin/$tool"
done
export PATH=$WORK/bin:$PATH
V5=com.tf2tools.app

# Writes a 100-byte SQLite header whose user_version (offset 60, big-endian) is $2, plus
# -wal and -shm files, into the v5 data dir under XDG data home $1.
fake_db() {
  local dir=$1/$V5
  mkdir -p "$dir"
  {
    printf 'SQLite format 3\000'
    head -c 44 /dev/zero
    printf "\\000\\000\\000\\$(printf '%03o' "$2")"
    head -c 36 /dev/zero
  } > "$dir/tf2tools.db"
  echo wal-bytes > "$dir/tf2tools.db-wal"
  echo shm-bytes > "$dir/tf2tools.db-shm"
}

install_sh() {
  local prefix=$1 data=$2
  shift 2
  TF2TOOLS_PREFIX=$prefix XDG_DATA_HOME=$data bash "$ROOT/scripts/install.sh" "$@"
}

backups() { find "$1/$V5" -maxdepth 1 -name 'backup-v5-*' | wc -l | tr -d ' '; }

# Waits up to 5 s for this shell's child named $1 to appear under that name; on a loaded machine
# fork and exec can take longer than any fixed sleep.
await_child() {
  local tries=0
  until pgrep -x -P "$$" "$1" > /dev/null; do
    tries=$((tries + 1))
    [[ $tries -lt 100 ]] || return 1
    sleep 0.05
  done
}

test_demo_dry_run_uses_demo_dirs_and_fake_tf2() {
  local cache=$WORK/demo-cache out base
  base=$cache/tf2tools-demo
  out=$(XDG_CACHE_HOME=$cache CARGO_TARGET_DIR=$WORK/target DEMO_DRY_RUN=1 \
    bash "$ROOT/scripts/demo.sh") || { fail "demo dry run failed: $out"; return 0; }
  local seed launch
  seed=$(printf '%s\n' "$out" | grep '^seed: ' || true)
  launch=$(printf '%s\n' "$out" | grep -v '^seed: ' || true)
  [[ $seed == *"$base/data/$V5 $base/Team Fortress 2"* ]] || fail "seed line [$seed]"
  [[ $launch == *"XDG_DATA_HOME=$base/data "* ]] || fail "data home in [$launch]"
  [[ $launch == *"XDG_CONFIG_HOME=$base/config "* ]] || fail "config home in [$launch]"
  [[ $launch == *"XDG_CACHE_HOME=$base/cache "* ]] || fail "cache home in [$launch]"
  [[ $launch == *"TF2TOOLS_ASSETS=$ROOT/assets "* ]] || fail "assets in [$launch]"
  [[ $launch == *" $WORK/target/release/tf2tools" ]] || fail "binary in [$launch]"
  [[ ! -e $base ]] || fail "dry run created $base"
}

test_demo_ignores_relative_cache_home() {
  local out
  out=$(cd "$WORK" && XDG_CACHE_HOME=rel DEMO_DRY_RUN=1 bash "$ROOT/scripts/demo.sh") || {
    fail "demo dry run failed"
    return 0
  }
  [[ $out == *"XDG_DATA_HOME=$HOME/.cache/tf2tools-demo/data "* ]] || fail "relative cache home used: $out"
}

test_install_backs_up_v5_db_once_and_leaves_originals() {
  local p=$WORK/i1/prefix d=$WORK/i1/data
  fake_db "$d" 9
  cp -R "$d/$V5" "$WORK/i1/orig"
  install_sh "$p" "$d" install > "$WORK/i1.log" 2>&1 || { fail "install failed: $(cat "$WORK/i1.log")"; return 0; }
  [[ $(backups "$d") == 1 ]] || fail "want one backup, have $(backups "$d")"
  local b
  b=$(find "$d/$V5" -maxdepth 1 -name 'backup-v5-*')
  for f in tf2tools.db tf2tools.db-wal tf2tools.db-shm; do
    cmp -s "$WORK/i1/orig/$f" "$d/$V5/$f" || fail "original $f changed"
    cmp -s "$WORK/i1/orig/$f" "$b/$f" || fail "backup $f differs"
  done
  grep -q "$b" "$WORK/i1.log" || fail "backup path not printed"
  grep -qi "v5 can still open it" "$WORK/i1.log" || fail "migration note not printed"
  install_sh "$p" "$d" install > /dev/null 2>&1 || fail "second install failed"
  [[ $(backups "$d") == 1 ]] || fail "second install made another backup"
}

test_install_skips_backup_for_a_v6_library() {
  local p=$WORK/i2/prefix d=$WORK/i2/data
  fake_db "$d" 10
  install_sh "$p" "$d" install > /dev/null 2>&1 || fail "install failed"
  [[ $(backups "$d") == 0 ]] || fail "backed up a v6 library"
}

test_install_refuses_while_tf2tools_runs() {
  local p=$WORK/i3/prefix d=$WORK/i3/data name=t2run$$
  fake_db "$d" 9
  # A shebang script's process name is its file name; the loop keeps bash from exec'ing sleep.
  printf '#!/bin/bash\nwhile :; do sleep 1; done\n' > "$WORK/bin/$name"
  chmod +x "$WORK/bin/$name"
  "$WORK/bin/$name" &
  local pid=$!
  if ! await_child "$name"; then
    fail "$name never started"
    kill "$pid" 2> /dev/null || true
    wait "$pid" 2> /dev/null || true
    return 0
  fi
  if TF2TOOLS_V5_PROCESS=$name install_sh "$p" "$d" install > "$WORK/i3.log" 2>&1; then
    fail "install ran while $name was running"
  fi
  kill "$pid" 2> /dev/null || true
  wait "$pid" 2> /dev/null || true
  grep -qi "running" "$WORK/i3.log" || fail "no running message: $(cat "$WORK/i3.log")"
  [[ $(backups "$d") == 0 ]] || fail "backed up while running"
  [[ ! -e $p/bin/tf2tools ]] || fail "installed while running"
}

test_install_sees_tf2tools_early_in_a_long_process_list() {
  local p=$WORK/i5/prefix d=$WORK/i5/data fake=$WORK/fakeps name=t2list$$
  fake_db "$d" 9
  mkdir -p "$fake"
  # A ps listing the app first and then a long tail: a reader that stops at the first match
  # leaves the writers to die of SIGPIPE, which pipefail turns into "not running".
  printf '#!/bin/sh\necho "/opt/x/%s"\nyes /usr/bin/other | head -n 200000\n' "$name" > "$fake/ps"
  chmod +x "$fake/ps"
  if PATH=$fake:$PATH TF2TOOLS_V5_PROCESS=$name install_sh "$p" "$d" install > "$WORK/i5.log" 2>&1; then
    fail "install ran while the process list named $name"
  fi
  grep -qi "running" "$WORK/i5.log" || fail "no running message: $(cat "$WORK/i5.log")"
  [[ $(backups "$d") == 0 ]] || fail "backed up while running"
  [[ ! -e $p/bin/tf2tools ]] || fail "installed while running"
}

test_install_writes_entry_and_uninstall_removes_only_it() {
  local p=$WORK/i4/prefix d=$WORK/i4/data
  mkdir -p "$p/bin" "$d/applications"
  echo keep > "$p/bin/other-tool"
  echo keep > "$d/applications/other.desktop"
  fake_db "$d" 9
  install_sh "$p" "$d" install > /dev/null 2>&1 || { fail "install failed"; return 0; }
  [[ -x $p/bin/tf2tools ]] || fail "no binary on PATH dir"
  [[ $("$p/bin/tf2tools") == stub ]] || fail "binary is not the prebuilt one"
  [[ -f $p/lib/tf2tools/assets/crosshairs/crosshairs.json ]] || fail "assets not beside the binary"
  local entry=$d/applications/$V5.desktop
  [[ -f $entry ]] || { fail "no desktop entry"; return 0; }
  grep -qx "Exec=\"$p/bin/tf2tools\"" "$entry" || fail "Exec line: $(grep Exec "$entry")"
  grep -qx 'Icon=tf2tools' "$entry" || fail "Icon line"
  grep -qx "StartupWMClass=$V5" "$entry" || fail "StartupWMClass line"
  [[ -f $d/icons/hicolor/512x512/apps/tf2tools.png ]] || fail "icon not installed"
  install_sh "$p" "$d" uninstall > /dev/null 2>&1 || fail "uninstall failed"
  [[ ! -L $p/bin/tf2tools ]] || fail "symlink left in bin"
  local left
  left=$(cd "$WORK/i4" && find . -type f | grep -v "/$V5/" | sort | tr '\n' ' ')
  [[ $left == "./data/applications/other.desktop ./prefix/bin/other-tool " ]] || fail "left behind: $left"
  [[ -f $d/$V5/tf2tools.db ]] || fail "uninstall touched the library"
  [[ $(backups "$d") == 1 ]] || fail "uninstall touched the backup"
}

test_make_without_target_installs() {
  local out
  out=$(env -u MAKEFLAGS -u MAKELEVEL -u MFLAGS make --no-print-directory -n \
    -f "$ROOT/Makefile" -C "$WORK" PREFIX="$WORK/mp") || { fail "make -n failed"; return 0; }
  [[ $out == *"TF2TOOLS_PREFIX=\"$WORK/mp\""*"scripts/install.sh install"* ]] || fail "make -n printed [$out]"
}

test_readme_shots_dry_run_drives_every_page_of_the_real_app() {
  local out
  out=$(README_SHOTS_DRY_RUN=1 bash "$ROOT/scripts/readme-screenshots.sh") || {
    fail "readme shots dry run failed: $out"
    return 0
  }
  local name
  for name in huds sounds crosshairs hud-store sound-store crosshair-store addon-store profiles settings hud-maker crosshair-maker; do
    [[ $out == *"\"nav-$name\""* ]] || fail "no nav-$name click in [$out]"
    [[ $out == *"# shot $name"* ]] || fail "no $name screenshot in [$out]"
  done
  [[ $(printf '%s\n' "$out" | grep -c '^screenshot$') == 11 ]] || fail "not eleven screenshots: [$out]"
  [[ $out != *XDG_* ]] || fail "launch.env refuses XDG names: [$out]"
  [[ $out == *'"mode": "headless"'* ]] || fail "not headless: [$out]"
  [[ $out == *'"build": false'* ]] || fail "lets the server build: [$out]"
  [[ $out == *'"TF2TOOLS_PILOT_REAL": "1"'* ]] || fail "not the real app: [$out]"
  [[ $out != *TF2TOOLS_PILOT_DATA* ]] || fail "swaps the real library for a data dir: [$out]"
  [[ $out == *'"crosshair-option:builtin:ql_3"'* ]] || fail "selects no crosshair: [$out]"
  [[ $out == *'"hud-maker-scene:main-menu"'* ]] || fail "HUD Maker not on the main menu: [$out]"
  [[ $out == *'"hud-maker-part:'* ]] || fail "selects no HUD part: [$out]"
  [[ $out == *'"crosshair-maker-starter-use"'* ]] || fail "draws no crosshair: [$out]"
  # The run's own changes: the sidebar opened and closed again, a temporary profile added and
  # deleted, and a HUD installed from the library.
  [[ $(printf '%s\n' "$out" | grep -c '"sidebar-toggle"') == 2 ]] || fail "the sidebar is not put back: [$out]"
  [[ $out == *'"profiles-search"}, "text": "Competitive"'* ]] || fail "no neutral profile filter: [$out]"
  # New profile selects its "Profile N" name, so the name is typed with no target (a target
  # clicks first and would append), and Back commits it.
  [[ $out == *'"profiles-new"'*'type {"text": "Competitive"}'*'"profile-editor-back"'* ]] || fail "no neutral profile: [$out]"
  [[ $out == *'"profiles-new"'*'wait {"target": {"id_suffix": "profile-editor-back"}'*'type {"text": "Competitive"}'* ]] || fail "types before the editor opens: [$out]"
  [[ $out != *profiles-save* ]] || fail "uses the removed Save Current: [$out]"
  [[ $out == *'"profile-delete:example-profile"'*'"confirm-ok"'*'# shot settings'* ]] || fail "the profile stays: [$out]"
  [[ $out == *'"hud-install:example-hud"'*'# shot huds'* ]] || fail "installs no HUD before the HUDs shot: [$out]"
  [[ $out != *'"hud-fonts-install"'* ]] || fail "installs fonts onto the machine: [$out]"
  # Clicking a weapon row assigns a crosshair, and these change the player's library or game.
  local banned
  for banned in weapon-row sound-play hud-delete crosshair-maker-save hud-maker-save hud-maker-backdrop settings-save; do
    [[ $out != *"$banned"* ]] || fail "touches $banned: [$out]"
  done
  [[ $(printf '%s\n' "$out" | tail -n 1) == close* ]] || fail "does not close the app: [$out]"
}

# Runs only the shot checks on shots dir $2 and records the outcome in $1.
shots_check() {
  README_SHOTS_CHECK=$2 bash "$ROOT/scripts/readme-screenshots.sh" > "$1" 2>&1
}

# Fills shots dir $1 with eleven empty shots and trees holding one on-screen label.
clean_shots() {
  local i
  mkdir -p "$1"
  for i in $(seq 11); do
    : > "$1/shot-$i.png"
    echo '{"nodes": [{"path": ["tf2-badge"], "name": "Team Fortress 2", "visible": true}]}' > "$1/tree-$i.json"
  done
}

test_readme_shots_check_fails_loudly_without_screen_trees() {
  local dir=$WORK/shots-notree i
  mkdir -p "$dir"
  for i in $(seq 11); do : > "$dir/shot-$i.png"; done
  if shots_check "$WORK/notree.out" "$dir"; then
    fail "passed with no tree-*.json: $(cat "$WORK/notree.out")"
  fi
  grep -q "no screen trees" "$WORK/notree.out" || fail "no clear message: $(cat "$WORK/notree.out")"
}

test_readme_shots_check_catches_a_home_path_and_passes_clean_shots() {
  local dir=$WORK/shots-clean
  clean_shots "$dir"
  shots_check "$WORK/clean.out" "$dir" || fail "clean shots failed: $(cat "$WORK/clean.out")"
  # A home path scrolled off screen is not in the picture.
  echo '{"nodes": [{"path": ["input-1"], "value": "/home/someone/tf", "visible": false}]}' > "$dir/tree-4.json"
  shots_check "$WORK/offscreen.out" "$dir" || fail "an off-screen path failed: $(cat "$WORK/offscreen.out")"
  # The installed HUD's folder path is painted over when the shot is converted.
  echo '{"nodes": [{"path": ["huds-installed-path"], "name": "/home/someone/tf/custom/x", "visible": true}]}' > "$dir/tree-4.json"
  shots_check "$WORK/blanked.out" "$dir" || fail "a blanked path failed: $(cat "$WORK/blanked.out")"
  echo '{"nodes": [{"path": ["input-1"], "value": "/home/someone/tf", "visible": true}]}' > "$dir/tree-4.json"
  if shots_check "$WORK/home.out" "$dir"; then
    fail "passed with a home path on screen"
  fi
  grep -q "home path" "$WORK/home.out" || fail "no clear message: $(cat "$WORK/home.out")"
}

test_readme_shots_check_catches_the_hud_maker_backdrop_and_a_missing_shot() {
  local dir=$WORK/shots-backdrop
  clean_shots "$dir"
  echo '{"nodes": [{"path": ["hud-maker-backdrop-image"], "visible": true}]}' > "$dir/tree-10.json"
  if shots_check "$WORK/backdrop.out" "$dir"; then
    fail "passed with the player's backdrop on screen"
  fi
  grep -q "backdrop" "$WORK/backdrop.out" || fail "no clear message: $(cat "$WORK/backdrop.out")"
  clean_shots "$dir"
  rm -f "$dir/shot-11.png"
  if shots_check "$WORK/missing.out" "$dir"; then
    fail "passed with ten screenshots"
  fi
  grep -q "expected 11 screenshots" "$WORK/missing.out" || fail "no clear message: $(cat "$WORK/missing.out")"
}

# Stubs magick, drive.py and a cargo that sleeps briefly, so a real run stops in its build step.
readme_shots_stubs() {
  local stubs=$1
  mkdir -p "$stubs/bin" "$stubs/pilot/scripts" "$stubs/tmp"
  printf '#!/bin/sh\nexit 0\n' > "$stubs/bin/magick"
  printf '#!/bin/sh\ntouch "%s/building"\nsleep 1\n' "$stubs" > "$stubs/bin/cargo"
  chmod +x "$stubs/bin/magick" "$stubs/bin/cargo"
  : > "$stubs/pilot/scripts/drive.py"
}

test_readme_shots_cleans_its_shots_root_on_term_and_hup() {
  local sig stubs pid i
  for sig in TERM HUP; do
    stubs=$WORK/shots-$sig
    readme_shots_stubs "$stubs"
    PATH=$stubs/bin:$PATH TMPDIR=$stubs/tmp CARGO=cargo GPUI_PILOT_REPO=$stubs/pilot \
      bash "$ROOT/scripts/readme-screenshots.sh" > "$stubs/out" 2>&1 &
    pid=$!
    for i in $(seq 50); do [[ -e $stubs/building ]] && break; sleep 0.1; done
    [[ -d $stubs/tmp/tf2tools-readme-shots ]] || fail "$sig: the run made no shots root"
    kill -s "$sig" "$pid"
    wait "$pid" 2> /dev/null || true
    [[ ! -e $stubs/tmp/tf2tools-readme-shots ]] || fail "$sig left the shots root behind"
  done
}

test_readme_shots_keeps_a_shots_root_it_did_not_make() {
  local stubs=$WORK/shots-existing
  readme_shots_stubs "$stubs"
  mkdir "$stubs/tmp/tf2tools-readme-shots"
  : > "$stubs/tmp/tf2tools-readme-shots/keep"
  if PATH=$stubs/bin:$PATH TMPDIR=$stubs/tmp CARGO=cargo GPUI_PILOT_REPO=$stubs/pilot \
    bash "$ROOT/scripts/readme-screenshots.sh" > "$stubs/out" 2>&1; then
    fail "ran over an existing shots root"
  fi
  [[ -f $stubs/tmp/tf2tools-readme-shots/keep ]] || fail "deleted a shots root it did not make"
}

# The package check's cheap failure paths: no directory, an empty one and two debs. No dir holds
# an AppImage, so the check stops at the deb and never executes anything.
test_package_check_fails_without_its_packages() {
  local empty=$WORK/pkg-empty two=$WORK/pkg-two dir rc out
  mkdir -p "$empty" "$two"
  : > "$two/tf2tools_6.0.0_amd64.deb"
  : > "$two/tf2tools_6.0.1_amd64.deb"
  for dir in "$WORK/pkg-missing" "$empty" "$two"; do
    out=$dir.out
    rc=0
    bash "$ROOT/scripts/test-package.sh" "$dir" > "$out" 2>&1 || rc=$?
    [[ $rc -eq 1 ]] || fail "test-package.sh $dir exited $rc: $(cat "$out")"
    grep -q '^FAIL: ' "$out" || fail "test-package.sh $dir printed no FAIL line: $(cat "$out")"
  done
}

for t in $(declare -F | awk '{print $3}' | grep '^test_'); do
  "$t"
done
if [[ $FAILS -gt 0 ]]; then
  echo "$FAILS failure(s)" >&2
  exit 1
fi
echo "scripts: all tests passed"
