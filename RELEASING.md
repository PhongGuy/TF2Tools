# Releasing TF2Tools

The maintainer's checklist for tagging, signing and publishing a release. Pushing a tag runs `.github/workflows/release.yml`, which builds the Linux packages and the Windows installer, signs `tf2tools-update.json` and stops at a draft release. Nothing is published until the maintainer publishes the draft.

Every step marked **(maintainer)** pushes, tags, publishes or handles the real signing key. Only the maintainer does those.

## Once, before the first release

### 1. Generate the signing key (maintainer)

Use the minisign CLI on a trusted machine:

```sh
minisign -G -p tf2tools-update.pub -s tf2tools-update.key
```

Give it a strong password when it asks. Never pass `-W`: an unencrypted key is refused by `make_manifest sign`, and the workflow needs the password secret anyway.

Keep `tf2tools-update.key` and its password offline (a password manager plus an offline backup). Neither ever goes into the repository.

### 2. Put the public key in the app (maintainer)

The second line of `tf2tools-update.pub` is the base64 public key (a `.pub` from `cargo packager signer generate` is base64-wrapped as a whole; decode it first). Put it in the first slot of `UPDATE_PUBLIC_KEYS` in `crates/tf2tools-core/src/update/keys.rs`:

```rust
pub const UPDATE_PUBLIC_KEYS: &[&str] = &["RW...the second line of tf2tools-update.pub..."];
```

Commit it (`feat(update): add the release signing key`). Every build from then on trusts manifests signed with this key.

### 3. Store the two secrets (maintainer)

The `draft` job's signing step reads two repository secrets, named as in the maintainer's other cargo-packager projects, and hands them to `make_manifest` as `TF2TOOLS_MINISIGN_KEY` and `TF2TOOLS_MINISIGN_PASSWORD`:

- `CARGO_PACKAGER_SIGN_PRIVATE_KEY`: the key file exactly as written, either the two-line minisign text or the single base64 line that `cargo packager signer generate` writes.
- `CARGO_PACKAGER_SIGN_PRIVATE_KEY_PASSWORD`: the key's password.

Set them with the GitHub CLI or on the repository's Settings page under Secrets and variables, Actions:

```sh
gh secret set CARGO_PACKAGER_SIGN_PRIVATE_KEY < tf2tools-update.key
gh secret set CARGO_PACKAGER_SIGN_PRIVATE_KEY_PASSWORD   # prompts for the value
```

### 4. Dry-run the real key locally (maintainer)

This proves the key file, its password and the compiled-in slot agree before CI ever sees them. It is the only check that a key made by the C minisign CLI parses in `make_manifest`.

```sh
make package
packages=${CARGO_TARGET_DIR:-target}/packages
read -rs TF2TOOLS_MINISIGN_PASSWORD; export TF2TOOLS_MINISIGN_PASSWORD
TF2TOOLS_MINISIGN_KEY="$(cat tf2tools-update.key)" \
  cargo run -p tf2tools-core --example make_manifest -- sign --dir "$packages" --version <version>
cargo run -p tf2tools-core --example make_manifest -- verify --dir "$packages"
unset TF2TOOLS_MINISIGN_PASSWORD
```

`<version>` is the workspace version in `Cargo.toml` (`[workspace.package]`). Run `verify` without `--key`, so it checks against `UPDATE_PUBLIC_KEYS`; it must print `signature ok`. `make package` writes to `$CARGO_TARGET_DIR/packages` when `CARGO_TARGET_DIR` is set and to `target/packages` otherwise (Makefile `PACKAGES`), hence `$packages`. Delete the generated `tf2tools-update.json` and `.minisig` from it afterwards.

### 5. Watch an old AppImage update itself (maintainer)

An automated run inside gpui-pilot showed that an AppImage finds, downloads, verifies and swaps in a newer one, but the pilot's sandbox killed the relauncher, so it could not show that the relaunch after the swap opens the new window. Check that by hand once, outside any sandbox:

1. Build two AppImages, an OLD one at the current version and a NEW one with the workspace version bumped (a scratch worktree works for the NEW one). Build each with `cargo build --release --locked -p tf2tools-app --bin tf2tools --features update-test-key` with `TF2TOOLS_RELEASE_BUILD` unset, then `NO_STRIP=1 cargo packager --release`. Never ship these builds.
2. Make a throwaway key with `cargo run -p tf2tools-core --example make_manifest -- keygen --dir <scratch>/key` (with a throwaway `TF2TOOLS_MINISIGN_PASSWORD` set; it writes `minisign.key` and `minisign.pub` and prints the public key). Sign the NEW AppImage with that key, which `sign` reads only from the environment:

   ```sh
   TF2TOOLS_MINISIGN_KEY="$(cat <scratch>/key/minisign.key)" TF2TOOLS_MINISIGN_PASSWORD=<throwaway password> \
     cargo run -p tf2tools-core --example make_manifest -- sign --dir <scratch>/new --version <new> --base http://127.0.0.1:8765
   ```

   Lay the signed files out under `<scratch>/feed/PhongGuy/TF2Tools/releases/download/v<new>/` and `<scratch>/feed/PhongGuy/TF2Tools/releases/latest/download/`, then serve the feed with `python3 -m http.server 8765 --bind 127.0.0.1` from `<scratch>/feed`.
3. Start the OLD AppImage from a terminal with scratch data dirs and the dead-proxy environment, so it touches neither your real library nor the real store hosts:

   ```sh
   (
   export XDG_DATA_HOME=<scratch>/data XDG_CONFIG_HOME=<scratch>/config XDG_CACHE_HOME=<scratch>/cache
   export HTTP_PROXY=http://127.0.0.1:9 HTTPS_PROXY=http://127.0.0.1:9 ALL_PROXY=http://127.0.0.1:9
   export http_proxy=$HTTP_PROXY https_proxy=$HTTPS_PROXY all_proxy=$ALL_PROXY
   export NO_PROXY=127.0.0.1,localhost no_proxy=127.0.0.1,localhost
   TF2TOOLS_UPDATE_TEST_PUBKEY=<throwaway pub> TF2TOOLS_UPDATE_TEST_BASE=http://127.0.0.1:8765 \
     <scratch>/app/tf2tools_<old>_x86_64.AppImage
   )
   ```

   The parentheses run it in a subshell, so these variables do not leak into later steps.

4. Update through the banner and press Restart. The old window must close and a NEW window must open by itself, showing the new version in Settings. If no window opens within about ten seconds, do not release until the relaunch is fixed.
5. Delete the scratch dir, including the throwaway key.

Any rerun of this loopback test sets `HTTP_PROXY`, `HTTPS_PROXY` and `ALL_PROXY` (and their lowercase forms) to `http://127.0.0.1:9` with `NO_PROXY=127.0.0.1,localhost` from the first launch. Without them the app syncs the stores from the real hosts.

## Each release

1. Bump `version` under `[workspace.package]` in `Cargo.toml` and commit it. A prerelease looks like `6.0.0-alpha.1`.
2. Run the local checks from a clean tree:

   ```sh
   make gate
   make package-check
   scripts/test-deb-prerelease.sh
   scripts/check-trailers.sh
   ```

   `scripts/test-deb-prerelease.sh` uses podman or docker when the host has no `dpkg-deb`. `scripts/check-trailers.sh` checks every commit since the previous tag for a `Co-authored-by:` or `Claude-Session:` trailer.
3. **(maintainer)** Push the branch.
4. **(maintainer)** Tag and push the tag:

   ```sh
   git tag -a v<version> -m "TF2Tools <version>"
   git push origin v<version>
   ```

   The tag must match the workspace version; the `draft` job fails early when it does not.
5. **(maintainer)** Watch the Release run (`gh run watch` or the Actions tab). It runs `gate`, then `linux` and `windows`, then `draft`, which calls `gh release create --draft`.
6. **(maintainer)** Review the draft:
   - The assets are the AppImage, the deb, `TF2Tools.exe`, `tf2tools-update.json` and `tf2tools-update.json.minisig`.
   - Download `TF2Tools.exe` on a Windows PC, install it and check it opens and finds TF2 before publishing.
   - Download the manifest and its signature and check them: `minisign -Vm tf2tools-update.json -p tf2tools-update.pub`.
   - A prerelease deb's Version uses `~`, such as `6.0.0~alpha.1`, so it sorts before `6.0.0`: `dpkg-deb -f tf2tools_<version>_amd64.deb Version`.
   - A tag with `-` in it is marked as a prerelease.
7. **(maintainer)** Write "what's new" in the draft body where the placeholder comment is.
8. **(maintainer)** Publish.

## Facts to keep in mind

- **Prereleases never reach the in-app updater.** The updater reads `releases/latest`, which skips prereleases, so the update feed starts with the first stable release.
- **`TF2Tools.exe` is a fixed name.** Every v4 client downloads `TF2Tools.exe` from the latest release, and the manifest's `windows-x86_64-nsis` entry points at it. The `windows` job renames cargo-packager's `tf2tools_<version>_x64-setup.exe` to it, and the `draft` job fails when it is missing from `dist` or from the verified manifest.
- **A bad published release is fixed by a higher version, never by deleting it.** Clients may already hold its manifest.
- **A run started by hand from a branch is a dry run.** The `draft` job runs only for `refs/tags/v*`, so a `workflow_dispatch` from a branch stops after uploading the `linux-packages` and `windows-installer` workflow artifacts.
- **The release build has no update test seam.** The `linux` and `windows` jobs set `TF2TOOLS_RELEASE_BUILD`, and the `update-test-key` feature refuses to compile with it set.
- **The trailer gate covers every commit since the previous tag.** The `gate` job runs `scripts/check-trailers.sh HEAD`, which fails on any `Co-authored-by:` or `Claude-Session:` line (any case) in that range; it must print `trailers OK`.
- **The packages bundle Valve assets.** The bundled crosshair VTFs, VMTs and PNGs and the weapon icons ship in every package, and the weapon scripts are compiled into `crates/tf2tools-core/src/weapons.rs`. Their licensing is the maintainer's call before the repository and its releases go public.

## The Windows installer

The `windows` job runs on `windows-2025` in Git for Windows' bash. It builds `tf2tools.exe` for the MSVC target with `AWS_LC_SYS_PREBUILT_NASM=1`, packages it with `cargo packager --formats nsis` (cargo-packager downloads NSIS itself), renames the installer to `TF2Tools.exe`, checks the exe carries the icon and uploads only `TF2Tools.exe`. It holds no secrets; `make_manifest` signs the installer in the `draft` job.

Locally, `scripts/check-windows.sh lint` and `scripts/check-windows.sh build` cross-check the gnu target in a container. The MSVC build is only ever seen in CI, so the first runs may fail in ways the local check cannot show:

- **The MSVC link.** gpui's manifest and the icon are two resource objects; the gnu link handles both, MSVC is assumed to.
- **Tests on Windows.** `ci.yml`'s `windows` job is the first run of the test suite, gpui's test platform included, on Windows. Fix each failure in its own commit that says why a test is gated or changed.
- **Git Bash tools.** The job uses `cygpath -m` and `grep -a` from Git for Windows' bash, assumed present on the runner image.

Before the first stable release, and again for any release that touches the installer or updater (maintainer):

1. Start `release.yml` by hand from the branch; it stops before the draft. Download the `windows-installer` artifact.
2. On a real Windows PC, run `TF2Tools.exe`. Expect "Windows protected your PC"; click "More info", then "Run anyway". It installs for the current user without an admin prompt.
3. Check the Start menu entry has the TF2Tools icon, the app opens, finds TF2 and its `tf/custom`, and installs a HUD.
4. Once a newer release is published, check the banner's update runs the installer, the app closes, and the new version opens.

v6 installs next to v4 in its own folder. Known and accepted: two Start menu entries, v6 replaces the desktop `TF2Tools.lnk`, and the v6 installer's running-app check also closes or asks about a running v4 window.

**Open decision (maintainer):** buy Authenticode code signing, or keep shipping unsigned with the "Windows protected your PC" step the release page explains. Decide before the first stable release.

## Key rotation (maintainer)

The updater reads only `releases/latest`, so a client that skips a release jumps straight to the newest one. A client that has never seen the new public key cannot verify a manifest signed with it, refuses the update and is stranded.

1. Generate the new key as in step 1 and put its public key in the second slot of `UPDATE_PUBLIC_KEYS`. Release that build (call it release N) signed with the old key, so clients learn the new key first.
2. Keep signing with the old key for several releases or a long period after N, so most clients pass through a build that trusts the new key.
3. Then replace both secrets with the new key and password and sign the next release with the new key. Clients older than N can no longer update in the app; they must download the new version from the Releases page by hand. Say so in that release's notes.
4. One release after the switch, drop the old key's slot.

If the old key is compromised, that wait is not possible: switch to the new key at once, and everyone older than N reinstalls by hand from the Releases page.

## Writing release notes

The release page is for players, not developers. The fixed text lives in `packaging/release-notes.md`; the "what's new" part is written in the draft.

- Use player words and say what changed in play terms ("Crosshairs now show a preview for every weapon").
- No commands, no file paths, no crate or library names.
- Keep it short: a few bullet points.
