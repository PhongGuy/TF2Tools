//! Builds, signs and checks `tf2tools-update.json`, the manifest the app's updater trusts.
//!
//! Usage:
//! `make_manifest sign --dir DIR --version VERSION [--base URL] [--notes TEXT] [--pub-date RFC3339]`
//! `make_manifest verify --dir DIR [--key BASE64]...`
//! `make_manifest keygen --dir DIR` (a throwaway encrypted key pair for local runs only)
//!
//! `sign` reads `TF2TOOLS_MINISIGN_KEY` and `TF2TOOLS_MINISIGN_PASSWORD` from the environment
//! and never prints either; `keygen` reads the password.

use std::io::{Cursor, Write};
use std::path::Path;
use std::process::ExitCode;

use chrono::{SecondsFormat, Utc};
use sha2::{Digest, Sha256};

const USAGE: &str = "usage: make_manifest sign --dir DIR --version VERSION [--base URL] \
                     [--notes TEXT] [--pub-date RFC3339]\n       \
                     make_manifest verify --dir DIR [--key BASE64]...\n       \
                     make_manifest keygen --dir DIR";
const KEY_VAR: &str = "TF2TOOLS_MINISIGN_KEY";
const PASSWORD_VAR: &str = "TF2TOOLS_MINISIGN_PASSWORD";
const MANIFEST: &str = "tf2tools-update.json";
const SIGNATURE: &str = "tf2tools-update.json.minisig";
const APPIMAGE_PLATFORM: &str = "linux-x86_64-appimage";
/// The release asset name of the Windows installer and its key, as core selects it on Windows.
const INSTALLER: &str = "TF2Tools.exe";
const INSTALLER_PLATFORM: &str = "windows-x86_64-nsis";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args, &|name| std::env::var(name).ok()) {
        Ok(out) => {
            println!("{out}");
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("make_manifest: {err}");
            ExitCode::FAILURE
        }
    }
}

/// Runs one subcommand with `env` standing in for the process environment, returning what
/// to print. Errors name the missing flag, variable or mismatch, never a secret's value.
fn run(args: &[String], env: &dyn Fn(&str) -> Option<String>) -> Result<String, String> {
    let (command, rest) = args.split_first().ok_or(USAGE)?;
    let flags = parse_flags(rest)?;
    match command.as_str() {
        "sign" => {
            only(
                &flags,
                &["--dir", "--version", "--base", "--notes", "--pub-date"],
            )?;
            let dir = Path::new(required(&flags, "--dir")?);
            let version = required(&flags, "--version")?;
            let pub_date = match single(&flags, "--pub-date")? {
                Some(date) => {
                    chrono::DateTime::parse_from_rfc3339(date)
                        .map_err(|e| format!("--pub-date {date:?} is not RFC 3339: {e}"))?;
                    date.to_owned()
                }
                None => Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
            };
            let json = build_manifest(
                dir,
                version,
                single(&flags, "--base")?.unwrap_or("https://github.com"),
                single(&flags, "--notes")?.unwrap_or(""),
                &pub_date,
            )?;
            // Both files are written only once the manifest has built and signed.
            let signature = sign_manifest(
                json.as_bytes(),
                env(KEY_VAR).as_deref(),
                env(PASSWORD_VAR).as_deref(),
            )?;
            let (manifest_path, signature_path) = (dir.join(MANIFEST), dir.join(SIGNATURE));
            write(&manifest_path, json.as_bytes(), false)?;
            write(&signature_path, signature.as_bytes(), false)?;
            let value: serde_json::Value =
                serde_json::from_str(&json).map_err(|e| e.to_string())?;
            let mut out = format!(
                "wrote {}\nwrote {}\nversion {version}",
                manifest_path.display(),
                signature_path.display(),
            );
            for (key, asset) in value["platforms"].as_object().into_iter().flatten() {
                out.push_str(&format!(
                    "\n{key}: sha256 {} size {}",
                    asset["sha256"].as_str().unwrap_or_default(),
                    asset["size"],
                ));
            }
            Ok(out)
        }
        "verify" => {
            only(&flags, &["--dir", "--key"])?;
            let dir = Path::new(required(&flags, "--dir")?);
            let given: Vec<&str> = values(&flags, "--key").collect();
            let keys = if given.is_empty() {
                tf2tools_core::UPDATE_PUBLIC_KEYS
            } else {
                &given
            };
            let json = read(&dir.join(MANIFEST))?;
            verify_manifest(&json, &read(&dir.join(SIGNATURE))?, keys)?;
            verify_files(dir, &json)
        }
        "keygen" => {
            only(&flags, &["--dir"])?;
            let dir = Path::new(required(&flags, "--dir")?);
            let password = env(PASSWORD_VAR)
                .filter(|p| !p.is_empty())
                .ok_or_else(|| format!("{PASSWORD_VAR} is not set"))?;
            let (secret_path, public_path) = (dir.join("minisign.key"), dir.join("minisign.pub"));
            // Refuse before the slow key derivation, so an existing pair is never clobbered.
            for path in [&secret_path, &public_path] {
                if path.exists() {
                    return Err(format!("{} already exists", path.display()));
                }
            }
            let (public, secret) = keygen(&password)?;
            let public_box = minisign::PublicKey::from_base64(&public)
                .and_then(|pk| pk.to_box())
                .map_err(|_| "could not encode the public key".to_owned())?
                .into_string();
            // The public half goes first, so a failed write never leaves a lone secret key.
            write(&public_path, public_box.as_bytes(), true)?;
            if let Err(err) = write_secret(&secret_path, secret.as_bytes()) {
                let _ = std::fs::remove_file(&public_path);
                return Err(err);
            }
            Ok(format!(
                "wrote {}\nwrote {}\npublic key {public}",
                secret_path.display(),
                public_path.display()
            ))
        }
        _ => Err(USAGE.to_owned()),
    }
}

/// The manifest JSON for the one `tf2tools_<version>_x86_64.AppImage` in `dir`, plus the
/// Windows installer when `TF2Tools.exe` is there too, each URL a release download under
/// `base` (trailing slash dropped). Refuses a version that is not plain semver (no leading
/// `v`) and a missing AppImage.
fn build_manifest(
    dir: &Path,
    version: &str,
    base: &str,
    notes: &str,
    pub_date: &str,
) -> Result<String, String> {
    semver::Version::parse(version)
        .map_err(|e| format!("version {version:?} is not a semver version: {e}"))?;
    let asset = |name: &str, bytes: &[u8]| {
        let url = format!(
            "{}/PhongGuy/TF2Tools/releases/download/v{version}/{name}",
            base.trim_end_matches('/')
        );
        serde_json::json!({ "url": url, "sha256": sha256_hex(bytes), "size": bytes.len() })
    };
    let name = format!("tf2tools_{version}_x86_64.AppImage");
    let bytes = std::fs::read(dir.join(&name))
        .map_err(|e| format!("cannot read {name} in {}: {e}", dir.display()))?;
    let mut platforms = serde_json::Map::new();
    platforms.insert(APPIMAGE_PLATFORM.to_owned(), asset(&name, &bytes));
    // The installer is optional, so a Linux-only release still signs; any other read
    // failure is refused rather than silently dropping the Windows entry.
    match std::fs::read(dir.join(INSTALLER)) {
        Ok(bytes) => {
            platforms.insert(INSTALLER_PLATFORM.to_owned(), asset(INSTALLER, &bytes));
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(format!("cannot read {INSTALLER} in {}: {e}", dir.display())),
    }
    let manifest = serde_json::json!({
        "version": version,
        "pub_date": pub_date,
        "notes": notes,
        "platforms": platforms,
    });
    serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())
}

/// A minisign signature over `manifest` with the encrypted secret key box `key`. Both secrets
/// must be present and non-empty; an unencrypted key is refused. Errors are fixed texts.
fn sign_manifest(
    manifest: &[u8],
    key: Option<&str>,
    password: Option<&str>,
) -> Result<String, String> {
    let key = key
        .filter(|k| !k.is_empty())
        .ok_or_else(|| format!("{KEY_VAR} is not set"))?;
    let password = password
        .filter(|p| !p.is_empty())
        .ok_or_else(|| format!("{PASSWORD_VAR} is not set"))?;
    // A SecretKeyBox is only parsed when converted, so each probe gets a fresh one. A key
    // that opens without a password is refused before any decryption is tried.
    let boxed = || minisign::SecretKeyBox::from(key.to_owned());
    if minisign::SecretKey::from_unencrypted_box(boxed()).is_ok() {
        return Err(format!(
            "{KEY_VAR} is not encrypted; the release key must have a password"
        ));
    }
    // A password is always passed: `None` would prompt on a terminal. Only a checksum
    // mismatch after decryption (kind Verify) means a wrong password.
    let secret =
        minisign::SecretKey::from_box(boxed(), Some(password.to_owned())).map_err(|e| {
            if matches!(e.kind(), minisign::ErrorKind::Verify) {
                "the signing key does not decrypt with that password".to_owned()
            } else {
                format!("{KEY_VAR} is not a minisign secret key")
            }
        })?;
    minisign::sign(None, &secret, Cursor::new(manifest), None, None)
        .map(|signature| signature.into_string())
        .map_err(|_| "signing the manifest failed".to_owned())
}

/// Ok when `signature` verifies over the exact `manifest` bytes with any of `keys` (base64
/// minisign public keys). Errors when `keys` is empty, a key or the signature does not
/// decode, or no key verifies.
fn verify_manifest(manifest: &[u8], signature: &[u8], keys: &[&str]) -> Result<(), String> {
    if keys.is_empty() {
        return Err(
            "no public keys: UPDATE_PUBLIC_KEYS is empty (pass --key for a local run)".into(),
        );
    }
    let keys = keys
        .iter()
        .map(|key| {
            minisign_verify::PublicKey::from_base64(key.trim())
                .map_err(|_| format!("{key:?} is not a minisign public key"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let signature = std::str::from_utf8(signature)
        .ok()
        .and_then(|text| minisign_verify::Signature::decode(text).ok())
        .ok_or("the signature is not a minisign signature")?;
    // Rotation: any configured key may have signed; legacy signatures are refused.
    keys.iter()
        .any(|key| key.verify(manifest, &signature, false).is_ok())
        .then_some(())
        .ok_or_else(|| "the signature does not verify with any of the keys".to_owned())
}

/// A fresh encrypted key pair: the base64 public key and the secret key box text.
fn keygen(password: &str) -> Result<(String, String), String> {
    let pair = minisign::KeyPair::generate_encrypted_keypair(Some(password.to_owned()))
        .map_err(|e| format!("key generation failed: {e}"))?;
    let secret = pair
        .sk
        .to_box(None)
        .map_err(|e| format!("could not encode the secret key: {e}"))?
        .into_string();
    Ok((pair.pk.to_base64(), secret))
}

/// Checks every platform's file (its URL's last segment) in `dir` against the manifest's
/// size, then sha256.
fn verify_files(dir: &Path, json: &[u8]) -> Result<String, String> {
    let value: serde_json::Value =
        serde_json::from_slice(json).map_err(|e| format!("{MANIFEST} is not JSON: {e}"))?;
    let platforms = value["platforms"]
        .as_object()
        .ok_or_else(|| format!("{MANIFEST} has no platforms"))?;
    let mut out = format!("signature ok for version {}", value["version"]);
    for (key, asset) in platforms {
        let name = asset["url"]
            .as_str()
            .and_then(|url| url.rsplit('/').next())
            .filter(|name| !name.is_empty() && *name != "." && *name != "..")
            .ok_or_else(|| format!("{key}: no file name in its url"))?;
        let bytes = read(&dir.join(name))?;
        if Some(bytes.len() as u64) != asset["size"].as_u64() {
            return Err(format!(
                "{key}: {name} size {} does not match the manifest",
                bytes.len()
            ));
        }
        if Some(sha256_hex(&bytes).as_str()) != asset["sha256"].as_str() {
            return Err(format!("{key}: {name} sha256 does not match the manifest"));
        }
        out.push_str(&format!("\n{key}: {name} matches"));
    }
    Ok(out)
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn read(path: &Path) -> Result<Vec<u8>, String> {
    std::fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))
}

/// Writes `bytes` to `path`; `fresh` refuses to replace an existing file.
fn write(path: &Path, bytes: &[u8], fresh: bool) -> Result<(), String> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true);
    if fresh {
        options.create_new(true);
    } else {
        options.create(true).truncate(true);
    }
    options
        .open(path)
        .and_then(|mut file| file.write_all(bytes))
        .map_err(|e| format!("cannot write {}: {e}", path.display()))
}

/// Writes `bytes` to a new file at `path` that only the owner can read (0600 on unix).
fn write_secret(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options
        .open(path)
        .and_then(|mut file| file.write_all(bytes))
        .map_err(|e| format!("cannot write {}: {e}", path.display()))
}

/// `--flag value` pairs in order; every flag needs a value.
fn parse_flags(rest: &[String]) -> Result<Vec<(&str, &str)>, String> {
    rest.chunks(2)
        .map(|pair| match pair {
            [flag, value] if flag.starts_with("--") => Ok((flag.as_str(), value.as_str())),
            _ => Err(format!("{USAGE}\n(bad argument {:?})", pair[0])),
        })
        .collect()
}

fn only(flags: &[(&str, &str)], allowed: &[&str]) -> Result<(), String> {
    match flags.iter().find(|(flag, _)| !allowed.contains(flag)) {
        Some((flag, _)) => Err(format!("{USAGE}\n(unknown flag {flag})")),
        None => Ok(()),
    }
}

fn values<'a>(flags: &'a [(&'a str, &'a str)], name: &'a str) -> impl Iterator<Item = &'a str> {
    flags
        .iter()
        .filter(move |(flag, _)| *flag == name)
        .map(|(_, value)| *value)
}

fn single<'a>(flags: &'a [(&'a str, &'a str)], name: &'a str) -> Result<Option<&'a str>, String> {
    let mut found = values(flags, name);
    let first = found.next();
    match found.next() {
        Some(_) => Err(format!("{name} given twice")),
        None => Ok(first),
    }
}

fn required<'a>(flags: &'a [(&'a str, &'a str)], name: &'a str) -> Result<&'a str, String> {
    single(flags, name)?.ok_or_else(|| format!("{USAGE}\n(missing {name})"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    use std::collections::HashMap;
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    use std::path::PathBuf;
    use std::sync::OnceLock;

    const VERSION: &str = "6.0.1";
    const APPIMAGE: &str = "tf2tools_6.0.1_x86_64.AppImage";
    const PASSWORD: &str = "throwaway-password";
    const GH: &str = "https://github.com";
    const DATE: &str = "2026-11-01T12:00:00Z";

    fn hex(bytes: &[u8]) -> String {
        Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    /// A packages dir as `make package` leaves it: both packages plus the packager's staging dir.
    fn packages() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(APPIMAGE), b"new appimage").unwrap();
        std::fs::write(dir.path().join("tf2tools_6.0.1_amd64.deb"), b"deb").unwrap();
        std::fs::create_dir(dir.path().join(".cargo-packager")).unwrap();
        dir
    }

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> + use<> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        move |name| map.get(name).cloned()
    }

    /// One encrypted key for the suite: each scrypt run takes about 2 s unoptimised.
    fn key() -> &'static (String, String) {
        static KEY: OnceLock<(String, String)> = OnceLock::new();
        KEY.get_or_init(|| keygen(PASSWORD).unwrap())
    }

    #[test]
    fn a_manifest_names_the_appimage_by_url_hash_and_size() {
        let dir = packages();
        let json = build_manifest(dir.path(), VERSION, GH, "Fixes.", DATE).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["version"], VERSION);
        assert_eq!(value["pub_date"], DATE);
        assert_eq!(value["notes"], "Fixes.");
        let platforms = value["platforms"].as_object().unwrap();
        assert_eq!(
            platforms.keys().collect::<Vec<_>>(),
            ["linux-x86_64-appimage"]
        );
        let asset = &platforms["linux-x86_64-appimage"];
        assert_eq!(
            asset["url"],
            format!("https://github.com/PhongGuy/TF2Tools/releases/download/v6.0.1/{APPIMAGE}")
        );
        assert_eq!(asset["sha256"], hex(b"new appimage"));
        assert_eq!(asset["size"], 12);
    }

    #[test]
    fn the_base_loses_a_trailing_slash() {
        let dir = packages();
        let json = build_manifest(dir.path(), VERSION, "http://127.0.0.1:8000/", "", DATE).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(
            value["platforms"]["linux-x86_64-appimage"]["url"],
            format!("http://127.0.0.1:8000/PhongGuy/TF2Tools/releases/download/v6.0.1/{APPIMAGE}")
        );
    }

    #[test]
    fn a_missing_appimage_or_a_bad_version_is_refused() {
        let dir = packages();
        let err = build_manifest(dir.path(), "6.0.2", GH, "", DATE).unwrap_err();
        assert!(err.contains("tf2tools_6.0.2_x86_64.AppImage"), "{err}");
        for bad in ["v6.0.1", "6.0", ""] {
            assert!(
                build_manifest(dir.path(), bad, GH, "", DATE).is_err(),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn a_signed_manifest_verifies_with_its_key_only() {
        let (public, secret) = key();
        let json = br#"{"version":"6.0.1","platforms":{}}"#;
        let signature = sign_manifest(json, Some(secret.as_str()), Some(PASSWORD)).unwrap();
        verify_manifest(json, signature.as_bytes(), &[public.as_str()]).unwrap();
        let mut tampered = json.to_vec();
        tampered[16] = b'2';
        assert!(verify_manifest(&tampered, signature.as_bytes(), &[public.as_str()]).is_err());
        let stranger = minisign::KeyPair::generate_unencrypted_keypair()
            .unwrap()
            .pk
            .to_base64();
        assert!(verify_manifest(json, signature.as_bytes(), &[stranger.as_str()]).is_err());
        // Rotation: any configured slot may hold the signing key.
        verify_manifest(
            json,
            signature.as_bytes(),
            &[stranger.as_str(), public.as_str()],
        )
        .unwrap();
    }

    #[test]
    fn verifying_with_no_keys_names_the_empty_slots() {
        let err = verify_manifest(b"{}", b"sig", &[]).unwrap_err();
        assert!(err.contains("UPDATE_PUBLIC_KEYS"), "{err}");
    }

    #[test]
    fn signing_needs_both_secrets_and_never_echoes_them() {
        let (_, secret) = key();
        let encoded = secret.lines().nth(1).unwrap();
        let cases = [
            (None, Some(PASSWORD), "TF2TOOLS_MINISIGN_KEY"),
            (Some(""), Some(PASSWORD), "TF2TOOLS_MINISIGN_KEY"),
            (Some(secret.as_str()), None, "TF2TOOLS_MINISIGN_PASSWORD"),
            (
                Some(secret.as_str()),
                Some(""),
                "TF2TOOLS_MINISIGN_PASSWORD",
            ),
        ];
        for (key, password, needle) in cases {
            let err = sign_manifest(b"{}", key, password).unwrap_err();
            assert!(err.contains(needle), "{err}");
        }
        let err = sign_manifest(b"{}", Some(secret.as_str()), Some("wrong-password")).unwrap_err();
        for leaked in [encoded, "wrong-password", PASSWORD] {
            assert!(!err.contains(leaked), "error echoes a secret: {err}");
        }
    }

    #[test]
    fn an_unencrypted_key_is_refused() {
        let pair = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
        let text = pair.sk.to_box(None).unwrap().into_string();
        assert_eq!(
            sign_manifest(b"{}", Some(&text), Some(PASSWORD)).unwrap_err(),
            "TF2TOOLS_MINISIGN_KEY is not encrypted; the release key must have a password"
        );
    }

    #[test]
    fn a_malformed_key_and_a_wrong_password_get_their_own_messages() {
        for text in ["not a key", "untrusted comment: x\n!!!not base64!!!\n"] {
            assert_eq!(
                sign_manifest(b"{}", Some(text), Some(PASSWORD)).unwrap_err(),
                "TF2TOOLS_MINISIGN_KEY is not a minisign secret key",
                "{text:?}"
            );
        }
        let (_, secret) = key();
        assert_eq!(
            sign_manifest(b"{}", Some(secret.as_str()), Some("wrong-password")).unwrap_err(),
            "the signing key does not decrypt with that password"
        );
    }

    #[test]
    fn sign_refuses_a_pub_date_that_is_not_rfc3339() {
        let dir = packages();
        let d = dir.path().to_str().unwrap();
        let (_, secret) = key();
        let secrets = env(&[
            ("TF2TOOLS_MINISIGN_KEY", secret.as_str()),
            ("TF2TOOLS_MINISIGN_PASSWORD", PASSWORD),
        ]);
        let err = run(
            &args(&["sign", "--dir", d, "--version", VERSION, "--pub-date", "x"]),
            &secrets,
        )
        .unwrap_err();
        assert!(err.contains("--pub-date"), "{err}");
        assert!(!dir.path().join("tf2tools-update.json").exists());
        run(
            &args(&["sign", "--dir", d, "--version", VERSION, "--pub-date", DATE]),
            &secrets,
        )
        .unwrap();
        let json = std::fs::read_to_string(dir.path().join("tf2tools-update.json")).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["pub_date"], DATE);
    }

    #[cfg(unix)]
    #[test]
    fn keygen_keeps_the_secret_key_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path().to_str().unwrap();
        let password = env(&[("TF2TOOLS_MINISIGN_PASSWORD", PASSWORD)]);
        run(&args(&["keygen", "--dir", d]), &password).unwrap();
        let mode = std::fs::metadata(dir.path().join("minisign.key"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn sign_and_verify_through_the_command_line() {
        let dir = packages();
        let d = dir.path().to_str().unwrap();
        let (public, secret) = key();
        let secrets = env(&[
            ("TF2TOOLS_MINISIGN_KEY", secret.as_str()),
            ("TF2TOOLS_MINISIGN_PASSWORD", PASSWORD),
        ]);
        let out = run(
            &args(&[
                "sign",
                "--dir",
                d,
                "--version",
                VERSION,
                "--notes",
                "Fixes.",
            ]),
            &secrets,
        )
        .unwrap();
        assert!(out.contains("tf2tools-update.json"), "{out}");
        assert!(!out.contains(secret.lines().nth(1).unwrap()), "{out}");
        let json = std::fs::read(dir.path().join("tf2tools-update.json")).unwrap();
        let signature = std::fs::read(dir.path().join("tf2tools-update.json.minisig")).unwrap();
        verify_manifest(&json, &signature, &[public.as_str()]).unwrap();
        run(
            &args(&["verify", "--dir", d, "--key", public.as_str()]),
            &env(&[]),
        )
        .unwrap();
        // Without --key the compiled-in slots decide, and a throwaway key is never among them.
        let err = run(&args(&["verify", "--dir", d]), &env(&[])).unwrap_err();
        if tf2tools_core::UPDATE_PUBLIC_KEYS.is_empty() {
            assert!(err.contains("UPDATE_PUBLIC_KEYS"), "{err}");
        }
    }

    #[test]
    fn verify_checks_the_files_against_the_manifest() {
        let dir = packages();
        let d = dir.path().to_str().unwrap();
        let (public, secret) = key();
        let secrets = env(&[
            ("TF2TOOLS_MINISIGN_KEY", secret.as_str()),
            ("TF2TOOLS_MINISIGN_PASSWORD", PASSWORD),
        ]);
        run(&args(&["sign", "--dir", d, "--version", VERSION]), &secrets).unwrap();
        let verify = args(&["verify", "--dir", d, "--key", public.as_str()]);
        std::fs::write(dir.path().join(APPIMAGE), b"old appimage").unwrap();
        let err = run(&verify, &env(&[])).unwrap_err();
        assert!(err.contains("sha256"), "{err}");
        std::fs::write(dir.path().join(APPIMAGE), b"old").unwrap();
        let err = run(&verify, &env(&[])).unwrap_err();
        assert!(err.contains("size"), "{err}");
    }

    #[test]
    fn an_installer_in_the_dir_adds_the_windows_entry() {
        let dir = packages();
        std::fs::write(dir.path().join("TF2Tools.exe"), b"installer").unwrap();
        let json = build_manifest(dir.path(), VERSION, GH, "", DATE).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        let platforms = value["platforms"].as_object().unwrap();
        assert_eq!(
            platforms.keys().collect::<Vec<_>>(),
            ["linux-x86_64-appimage", "windows-x86_64-nsis"]
        );
        let asset = &platforms["windows-x86_64-nsis"];
        assert_eq!(
            asset["url"],
            "https://github.com/PhongGuy/TF2Tools/releases/download/v6.0.1/TF2Tools.exe"
        );
        assert_eq!(asset["sha256"], hex(b"installer"));
        assert_eq!(asset["size"], 9);
    }

    #[test]
    fn verify_checks_the_installer_too() {
        let dir = packages();
        std::fs::write(dir.path().join("TF2Tools.exe"), b"installer").unwrap();
        let d = dir.path().to_str().unwrap();
        let (public, secret) = key();
        let secrets = env(&[
            ("TF2TOOLS_MINISIGN_KEY", secret.as_str()),
            ("TF2TOOLS_MINISIGN_PASSWORD", PASSWORD),
        ]);
        let signed = run(&args(&["sign", "--dir", d, "--version", VERSION]), &secrets).unwrap();
        assert!(signed.contains("windows-x86_64-nsis"), "{signed}");
        let verify = args(&["verify", "--dir", d, "--key", public.as_str()]);
        let ok = run(&verify, &env(&[])).unwrap();
        assert!(
            ok.contains("windows-x86_64-nsis: TF2Tools.exe matches"),
            "{ok}"
        );
        // Same length, so the size passes and the hash must catch it.
        std::fs::write(dir.path().join("TF2Tools.exe"), b"tampered!").unwrap();
        let err = run(&verify, &env(&[])).unwrap_err();
        assert!(err.contains("TF2Tools.exe sha256 does not match"), "{err}");
    }

    #[test]
    fn sign_without_secrets_writes_nothing() {
        let dir = packages();
        let d = dir.path().to_str().unwrap();
        let err = run(
            &args(&["sign", "--dir", d, "--version", VERSION]),
            &env(&[]),
        )
        .unwrap_err();
        assert!(err.contains("TF2TOOLS_MINISIGN_KEY"), "{err}");
        assert!(!dir.path().join("tf2tools-update.json").exists());
        assert!(!dir.path().join("tf2tools-update.json.minisig").exists());
    }

    #[test]
    fn keygen_writes_an_encrypted_pair_once() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path().to_str().unwrap();
        let err = run(&args(&["keygen", "--dir", d]), &env(&[])).unwrap_err();
        assert!(err.contains("TF2TOOLS_MINISIGN_PASSWORD"), "{err}");
        let password = env(&[("TF2TOOLS_MINISIGN_PASSWORD", PASSWORD)]);
        let out = run(&args(&["keygen", "--dir", d]), &password).unwrap();
        let public = std::fs::read_to_string(dir.path().join("minisign.pub")).unwrap();
        let public_line = public.lines().nth(1).unwrap();
        assert_eq!(public_line.len(), 56);
        assert!(out.contains(public_line), "{out}");
        let secret = std::fs::read_to_string(dir.path().join("minisign.key")).unwrap();
        assert!(secret.starts_with("untrusted comment:"), "{secret}");
        let boxed = minisign::SecretKeyBox::from_string(&secret).unwrap();
        assert!(boxed.into_secret_key(Some(PASSWORD.to_owned())).is_ok());
        let err = run(&args(&["keygen", "--dir", d]), &password).unwrap_err();
        assert!(err.contains("exists"), "{err}");
    }

    /// Serves what `make_manifest` signs for `dir` from a loopback feed and returns the core
    /// client's offer to `target` on this OS, plus the base its asset URLs sit under.
    #[cfg(target_arch = "x86_64")]
    async fn core_offer(
        dir: &Path,
        target: &tf2tools_core::InstallTarget,
    ) -> (tf2tools_core::UpdateOffer, String) {
        use tf2tools_core::{Endpoints, StoreConfig, UpdateClient};
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        let (public, secret) = key();
        // Endpoints::loopback serves github under /gh (tests/update_check.rs).
        let base = format!("{}/gh", server.uri());
        let json = build_manifest(dir, VERSION, &base, "Fixes.", DATE).unwrap();
        let signature =
            sign_manifest(json.as_bytes(), Some(secret.as_str()), Some(PASSWORD)).unwrap();
        let feed = "/gh/PhongGuy/TF2Tools/releases/latest/download/tf2tools-update.json";
        Mock::given(method("GET"))
            .and(path(feed))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(json.into_bytes()))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(format!("{feed}.minisig")))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(signature.into_bytes()))
            .mount(&server)
            .await;
        let client = UpdateClient::new(
            &Endpoints::loopback(&server.uri()),
            &StoreConfig::default(),
            &[public.as_str()],
        )
        .unwrap();
        let offer = client
            .check("6.0.0", target)
            .await
            .unwrap()
            .expect("an offer");
        assert_eq!(offer.version(), VERSION);
        (offer, base)
    }

    // An installer beside the AppImage must not disturb what a Linux install is offered.
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    #[tokio::test]
    async fn the_core_client_accepts_what_make_manifest_signs() {
        let dir = packages();
        std::fs::write(dir.path().join("TF2Tools.exe"), b"installer").unwrap();
        let target =
            tf2tools_core::InstallTarget::AppImage(PathBuf::from("/apps/tf2tools.AppImage"));
        let (offer, base) = core_offer(dir.path(), &target).await;
        let asset = offer.asset().unwrap();
        assert_eq!(
            asset.url(),
            format!("{base}/PhongGuy/TF2Tools/releases/download/v6.0.1/{APPIMAGE}")
        );
        assert_eq!(asset.sha256(), hex(b"new appimage"));
        assert_eq!(asset.size(), 12);
    }

    // The core client picks its key from the running OS, so only a Windows run (CI) can
    // prove an installed copy selects the entry `sign` writes.
    #[cfg(all(windows, target_arch = "x86_64"))]
    #[tokio::test]
    async fn the_core_client_offers_the_installer_to_a_windows_install() {
        let dir = packages();
        std::fs::write(dir.path().join("TF2Tools.exe"), b"installer").unwrap();
        let target = tf2tools_core::InstallTarget::WindowsInstaller;
        let (offer, base) = core_offer(dir.path(), &target).await;
        let asset = offer.asset().unwrap();
        assert_eq!(
            asset.url(),
            format!("{base}/PhongGuy/TF2Tools/releases/download/v6.0.1/TF2Tools.exe")
        );
        assert_eq!(asset.sha256(), hex(b"installer"));
        assert_eq!(asset.size(), 9);
    }
}
