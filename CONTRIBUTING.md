# Contributing to TF2Tools

This is the developer guide; the player-facing page is [README.md](README.md).

## Requirements

- Rust 1.99 or newer (the `rust-version` in `Cargo.toml`).
- On Debian or Ubuntu, the system libraries CI installs:

```bash
sudo apt-get install -y --no-install-recommends pkg-config libfontconfig-dev libfreetype-dev \
  libwayland-dev libx11-xcb-dev libxkbcommon-x11-dev libvulkan1 libegl1 libasound2-dev \
  mesa-vulkan-drivers
```

## Try it and install it

```bash
make demo        # seeded throwaway library and a fake TF2 install; the stores stay online
make             # same as `make install`: builds a release and installs it for your user
make uninstall   # removes the binary, desktop entry and icon; never your library
```

`make demo` keeps everything in `~/.cache/tf2tools-demo` (fresh each run; `DEMO_KEEP=1` reuses it), so it never touches your real library or game. Its installs land in a fake `Team Fortress 2/tf/custom` there.

The demo starts the way the real app does, so on Linux its HUD font check runs `fc-list` against your machine's real font set: whether a HUD's fonts dialog appears depends on the fonts you have installed, so two machines can show different results for the same HUD. Fonts the demo installs go to `~/.cache/tf2tools-demo/data/fonts/tf2tools` and are removed with the rest of the demo, never into your own font folder.

`make install` puts the binary and its assets in `~/.local/lib/tf2tools`, links it as `~/.local/bin/tf2tools` and adds a desktop entry. `make PREFIX=/some/dir` uses `/some/dir/lib/tf2tools` and `/some/dir/bin` instead. **Coming from v5:** v6 upgrades a v5 library to schema 10 on its first launch. As a precaution, install first copies the database into a dated `backup-v5-*` folder beside it and tells you where. It refuses while TF2Tools is running; close it and run `make` again.

## Linux packages

To build the Linux packages instead (needs `cargo-packager` 0.11.8):

```bash
make package        # deb and AppImage in <target dir>/packages, or PACKAGES=/some/dir
make package-check  # the same, then checks both packages' layout without installing them
```

They are named `tf2tools_<version>_amd64.deb` and `tf2tools_<version>_x86_64.AppImage`. Release packages are built on ubuntu 22.04, since a newer build host raises the glibc floor.

## Develop

```bash
cargo run -p tf2tools-app   # runs against your real library; prefer `make demo`
make check                  # the CI gate: fmt, clippy, and the tests with no display
make gate                   # `make check` plus the pilot crate and the script tests
make fmt                    # format the workspace
```

`cargo run` uses the real library in `~/.local/share/com.tf2tools.app`, which is why `make demo` is the safer way to try a change.

## README screenshots

```bash
make readme-shots   # rebuilds the fourteen README screenshots in .github/readme
```

`make readme-shots` runs `scripts/readme-screenshots.sh`. It needs the gpui-pilot server (`GPUI_PILOT`, else `gpui-pilot` on your PATH), a gpui-pilot checkout for its `scripts/drive.py` (`GPUI_PILOT_REPO`, default `~/git/gpui-pilot`) and ImageMagick's `magick`. It builds the pilot crate, then drives the real app (`TF2TOOLS_PILOT_REAL=1`) through fourteen pages against your own library, TF2 install and the online stores, and rewrites every image in `.github/readme/` as lossless WebP.

The run changes little and puts back what it can. It opens the sidebar so the shots show its labels, and closes it again at the end if it was closed. If no HUD is installed it installs your first library HUD into the game, skipping any fonts it offers; that stays installed. It saves your current setup as a "Competitive" profile, filters the Profiles page to it for the shot and deletes it, and refuses to run if a profile matching that name exists. It waits up to thirty minutes each for the sound and crosshair stores to finish syncing, up to twenty for the GameBanana maps catalogue (starting that sync if auto-sync is off) and up to ten for your logs.tf logs. Otherwise it only looks: it selects a crosshair on the Crosshairs page but never clicks a weapon row (that assigns a crosshair), draws a starter in the Crosshair Maker but never saves it, and shows the HUD Maker's main menu, which draws the game's own menu picture instead of your backdrop screenshot. Settings is scrolled past the version card and your TF2 folder's path, the installed HUD's folder path and the logs.tf player chips, which name you, are painted over, no logs.tf log is opened, and the Sound Store shot loses its bottom strip so the volume bar covers no card. Before converting, it fails if any other on-screen label holds a home path or the HUD Maker backdrop shows. If it stops early it deletes the profile and closes the sidebar first. Look at every image before committing it: names from your library end up in the README.

### Real-app screenshots and QA

By default the pilot never sees your own library or game. To check something against them, or to take screenshots of the app as you really use it, launch it with `TF2TOOLS_PILOT_REAL=1`:

```text
launch {"mode": "headless", "env": {"TF2TOOLS_PILOT_REAL": "1", "TF2TOOLS_ASSETS": "<repo>/assets"}}
```

The pilot then starts the app exactly as the installed app starts: your normal library, your Steam and TF2 install, the online stores and the update check. Nothing is seeded and nothing is faked, so whatever you click really happens: installs write into your game's `tf/custom` and store syncs reach the network. `TF2TOOLS_PILOT_DATA` swaps your normal library for that data dir (the only way to keep one, since the host hides `XDG_DATA_HOME`); `TF2TOOLS_PILOT_EMPTY_STORE` does nothing in this mode; `TF2TOOLS_PILOT_REDUCE_MOTION` still works. It uses your home from the system's user list and assumes the usual `~/.local/share` and `~/.cache` layout, since the pilot's host hides your own `HOME` and `XDG_*` settings. File dialogs still open inside the pilot, where its `pick_file` answers them, never on your desktop. Leave the variable out and the pilot runs on its seeded sample library as before.

## Pilot QA

The pilot crate (`tools/tf2tools-pilot`) runs the app under [gpui-pilot](https://github.com/PhongGuy/gpui-pilot) v0.1.0 against a seeded offline library. Install the matching server with:

```bash
cargo install --locked --git https://github.com/PhongGuy/gpui-pilot --tag v0.1.0 gpui-pilot
```

Its launch reads these variables:

- `TF2TOOLS_PILOT_DATA`: the data dir to seed and use, instead of the launch's own temp dir. Pass it in the launch's `env` (`launch {"mode": "headless", "env": {"TF2TOOLS_PILOT_DATA": "<dir>"}}`): the pilot host starts with a cleared environment, so a variable exported in your shell never reaches the app, and `XDG_*_HOME` and `HOME` are refused in `env` because the pilot owns them. Session mode runs the shipped app, which ignores this variable.
- `TF2TOOLS_PILOT_REDUCE_MOTION=1`: turns on reduced motion, which a headless run cannot read from the system.
- `TF2TOOLS_PILOT_EMPTY_STORE`: one of `curated`, `huds_gb`, `sounds`, `crosshairs` or `addons`; the seed leaves that store with no items and no sync record, so its loading cards show. `tf2tools-seed` reads it too. It only applies when the seed runs, so point `TF2TOOLS_PILOT_DATA` at a fresh, empty data dir.
- `TF2TOOLS_PILOT_GRAPHICS=1`: the fake TF2 also gets mastercomfig 9.100.1, managed by TF2Tools on the High preset with its whole settings list cached, and four settings changed from High (vsync on, motion blur high, FPS cap 144 and decals left to TF2), so the Graphics page shows its rows, changed bars and Reset offline. `tf2tools-seed` reads it too, and it also adds mastercomfig to a data dir seeded earlier.
- Session mode runs the real app (`launch {"mode": "session", "network": "pasta"}` for online stores). `gpui-pilot.toml` binds the `assets` folder into its sandbox; pass `"env": {"TF2TOOLS_ASSETS": "<repo>/assets"}` so the app finds it there.
- Session mode only sees AT-SPI accessibility ids, so the app sets them with `accessibility_id(...)`, using the same string as the element id (`nav-crosshairs`, `store-card:<store>:<id>`). The sidebar items and the HUD, Sound, Addon, Profile and store cards carry them; click one by id or by id suffix. Headless runs keep using the element ids.

The seed's Arena profile names a store HUD that is not in the library, for checking Missing Items.

## Translations

The text TF2Tools shows lives in `crates/tf2tools-app/locales/`, one file per language. `en.yml` is the source and the fallback: a key a language lacks shows in English.

To add a language:

1. Pick the code. The file name without `.yml` is the locale code. Use a bare language code, such as `es`, `de` or `fr`, when one file serves every region, and a region tag the UI kit uses, such as `pt-BR` or `zh-CN`, when the variants differ; gpui-component's `locales/ui.yml` lists the kit's codes.
2. Copy `en.yml` to `<code>.yml` and translate the values, never the keys. The file must keep exactly the keys of `en.yml`, and the tests check it.
3. Add the code and its native name with " (preview)" to `LANGUAGES` in `crates/tf2tools-app/src/i18n.rs`, as in `Português do Brasil (preview)`, and add the same pair to the pinned list in the test `english_comes_first_and_every_catalogue_is_offered` in that file.
4. Add a plural rule to `plural_form` in the same file only if the language counts differently from `one` for 1 and `other` for everything else.
5. Run `cargo test -p tf2tools-app --lib i18n::` and `cargo test -p tf2tools-app --test i18n`, then open a PR.

A system locale picks a language by its full tag first, then by its language alone: `pt_BR.UTF-8` and `pt-PT` both start TF2Tools in `pt-BR`, `es_ES.UTF-8` and `es_MX` both start TF2Tools in `es`, `de_DE.UTF-8` and `de_AT` both start TF2Tools in `de`, and `fr_FR.UTF-8` and `fr_CA` both start TF2Tools in `fr`. `zh_CN.UTF-8` and `zh-Hans` start TF2Tools in `zh-CN`, while Traditional Chinese tags such as `zh_TW`, `zh_HK` and `zh-Hant` are skipped, so such a system starts in its next language or in English.

Rules for the files:

- Settle the recurring words in a glossary comment at the top of the file, as `pt-BR.yml` does, and keep game terms as the TF2 client shows them in that language.
- Wrap every value in double quotes, because YAML reads a bare `Yes`, `No` or `Off` as true or false.
- Keep every `%{name}` exactly as it is; the tests fail a translation that drops or renames one.
- Plural keys end in `one`, `few`, `many` or `other`, and a file has exactly the forms its language uses: English `one` and `other`, Russian `one`, `few` and `many`. Brazilian Portuguese, Spanish and German have `one` and `other` too, and zero takes `other` ("0 HUDs"). French also has `one` and `other`, but its `one` covers zero ("0 HUD"). Simplified Chinese has only `other`, one text for every count.
- A string literal shaped like a catalogue key (for example "settings.json") fails the key-existence test, so section names must not collide with ordinary dotted text.
- Dates, numbers and sizes come from the keys under `common.date`, `common.number` and `common.size`. Reorder `%{day}`, `%{month}` and `%{year}` instead of writing a date pattern, and write a no-break space as `"\u00A0"` inside double quotes. Compact counts come from `common.count`, whose `step` is the size of one tier: `1000` for thousands, `10000` for Chinese 万.

A language stays "(preview)" in the picker until a native speaker has reviewed it.

After a language switch, some text, such as an open list, changes the next time it opens. A message already on screen stays in the language it was shown in until it closes.

## Layout

- `crates/tf2tools-core`: the UI-free core (library, installs, file formats, store clients, sync).
- `crates/tf2tools-app`: the GPUI app.
- `assets/`: bundled crosshairs, weapon icons and backgrounds.
- `packaging/`: the icons and the desktop entry the Linux packages ship.
- `tools/tf2tools-pilot`: the gpui-pilot driver for visual QA, outside the workspace.
- `vendor/`: patched upstream crates.

## Releases

How a release is cut and checked is in [RELEASING.md](RELEASING.md).
