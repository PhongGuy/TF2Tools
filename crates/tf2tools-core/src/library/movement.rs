//! Null-Canceling Movement's key binds: the player's movement keys as TF2's `config.cfg`
//! binds them, and putting the stock movement commands back when the addon goes.

use std::io;
use std::path::{Path, PathBuf};

use super::{IoFailure, io_at};

/// The asset name of mastercomfig's Null-Canceling Movement addon.
pub(crate) const NULL_MOVEMENT: &str = "mastercomfig-addon-null-canceling-movement.vpk";

/// One of the four movement directions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum MoveDirection {
    /// `+forward`, or the addon's `+mf`.
    Forward,
    /// `+back`, or `+mb`.
    Back,
    /// `+moveleft`, or `+ml`.
    Left,
    /// `+moveright`, or `+mr`.
    Right,
}

impl MoveDirection {
    /// Forward, back, left, right.
    pub const ALL: [MoveDirection; 4] = [Self::Forward, Self::Back, Self::Left, Self::Right];

    fn index(self) -> usize {
        self as usize
    }

    /// The stock command, as `config.cfg` binds it without the addon.
    fn stock(self) -> &'static str {
        ["+forward", "+back", "+moveleft", "+moveright"][self.index()]
    }

    /// The addon's command for this direction.
    pub(crate) fn null_command(self) -> &'static str {
        ["+mf", "+mb", "+ml", "+mr"][self.index()]
    }

    /// The direction a bound command moves in, stock or the addon's; None for anything else.
    fn of_command(command: &[u8]) -> Option<Self> {
        Self::ALL.into_iter().find(|d| {
            command.eq_ignore_ascii_case(d.stock().as_bytes())
                || command.eq_ignore_ascii_case(d.null_command().as_bytes())
        })
    }
}

/// The keys bound to each movement direction, in the order `config.cfg` binds them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct MovementKeys {
    keys: [Vec<String>; 4],
}

impl MovementKeys {
    /// The keys that move the player in `direction`; empty when none does.
    pub fn keys(&self, direction: MoveDirection) -> &[String] {
        &self.keys[direction.index()]
    }

    /// The directions no key is bound to.
    pub fn missing(&self) -> Vec<MoveDirection> {
        MoveDirection::ALL
            .into_iter()
            .filter(|d| self.keys(*d).is_empty())
            .collect()
    }

    /// True when no direction has a key.
    pub fn is_empty(&self) -> bool {
        self.keys.iter().all(Vec::is_empty)
    }
}

/// The movement keys `config` binds.
pub(crate) fn movement_keys(config: &str) -> MovementKeys {
    // Replay the binds in order, as TF2 does, so a key bound again, unbound or cleared by
    // `unbindall` keeps only what it ends up as. Keys compare without case, as TF2's do.
    let mut bound: Vec<(String, MoveDirection)> = Vec::new();
    for line in config.lines() {
        let Some(words) = tokens(line.as_bytes()) else {
            continue;
        };
        let word = |i: usize| words.get(i).map(|r| &line.as_bytes()[r.clone()]);
        let Some(command) = word(0) else { continue };
        if command.eq_ignore_ascii_case(b"unbindall") {
            bound.clear();
            continue;
        }
        let rebinds = command.eq_ignore_ascii_case(b"bind") && words.len() >= 3;
        if !(rebinds || command.eq_ignore_ascii_case(b"unbind")) {
            continue;
        }
        let Some(key) = word(1).and_then(|k| std::str::from_utf8(k).ok()) else {
            continue;
        };
        let direction = (rebinds && words.len() == 3)
            .then(|| word(2).and_then(MoveDirection::of_command))
            .flatten();
        let at = bound.iter().position(|(k, _)| k.eq_ignore_ascii_case(key));
        match (at, direction) {
            (Some(i), Some(d)) => bound[i] = (key.to_owned(), d),
            (Some(i), None) => drop(bound.remove(i)),
            (None, Some(d)) if safe_key(key) => bound.push((key.to_owned(), d)),
            (None, _) => {}
        }
    }
    let mut keys = MovementKeys::default();
    for (key, direction) in bound {
        keys.keys[direction.index()].push(key);
    }
    keys
}

/// Whether `key` can sit inside a quoted `bind` without breaking the line.
fn safe_key(key: &str) -> bool {
    !key.is_empty() && !key.contains(['"', ';', '\r', '\n'])
}

/// The byte ranges of a cfg line's words, quotes left out, up to a `//` comment. None when
/// the line runs several commands (a `;` outside quotes) or leaves a quote open.
fn tokens(line: &[u8]) -> Option<Vec<std::ops::Range<usize>>> {
    let mut words = Vec::new();
    let mut i = 0;
    while i < line.len() {
        match line[i] {
            b if b.is_ascii_whitespace() => i += 1,
            b'/' if line.get(i + 1) == Some(&b'/') => break,
            b';' => return None,
            b'"' => {
                let len = line[i + 1..].iter().position(|&b| b == b'"')?;
                words.push(i + 1..i + 1 + len);
                i += len + 2;
            }
            _ => {
                let len = line[i..]
                    .iter()
                    .position(|&b| b.is_ascii_whitespace() || b == b'"' || b == b';')
                    .unwrap_or(line.len() - i);
                words.push(i..i + len);
                i += len;
            }
        }
    }
    Some(words)
}

/// `config` with each bind to `+mf/+mb/+ml/+mr` back on the stock command; None when no
/// line changed. Only the command word changes, so quotes, comments and line ends stay.
pub(crate) fn restore_stock_binds(config: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(config.len() + 64);
    let mut changed = false;
    for line in config.split_inclusive(|&b| b == b'\n') {
        let body = line.strip_suffix(b"\n").unwrap_or(line);
        let body = body.strip_suffix(b"\r").unwrap_or(body);
        let swap = tokens(body).and_then(|words| {
            let [first, _, command] = words.as_slice() else {
                return None;
            };
            let command = command.clone();
            let direction = MoveDirection::ALL.into_iter().find(|d| {
                body[command.clone()].eq_ignore_ascii_case(d.null_command().as_bytes())
            })?;
            body[first.clone()]
                .eq_ignore_ascii_case(b"bind")
                .then_some((command, direction))
        });
        match swap {
            Some((command, direction)) => {
                out.extend_from_slice(&line[..command.start]);
                out.extend_from_slice(direction.stock().as_bytes());
                out.extend_from_slice(&line[command.end..]);
                changed = true;
            }
            None => out.extend_from_slice(line),
        }
    }
    changed.then_some(out)
}

fn config_path(tf_dir: &Path) -> PathBuf {
    tf_dir.join("cfg").join("config.cfg")
}

/// The movement keys `tf/cfg/config.cfg` binds; none when the file is missing.
pub(crate) fn read_movement_keys(tf_dir: &Path) -> Result<MovementKeys, IoFailure> {
    let path = config_path(tf_dir);
    match std::fs::read(&path) {
        Ok(bytes) => Ok(movement_keys(&String::from_utf8_lossy(&bytes))),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(MovementKeys::default()),
        Err(e) => Err(io_at(&path)(e)),
    }
}

/// Puts the stock movement commands back in `tf/cfg/config.cfg`; true when it changed.
/// The new text goes to a temp file first and is renamed over, so a failed write never
/// leaves the player with half a config.
pub(crate) fn restore_config(tf_dir: &Path) -> Result<bool, IoFailure> {
    let path = config_path(tf_dir);
    let bytes = match std::fs::read(&path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(io_at(&path)(e)),
    };
    let Some(restored) = restore_stock_binds(&bytes) else {
        return Ok(false);
    };
    let temp = path.with_file_name("config.cfg.tf2tools-tmp");
    std::fs::write(&temp, restored).map_err(io_at(&temp))?;
    if let Err(e) = std::fs::rename(&temp, &path) {
        let _ = std::fs::remove_file(&temp);
        return Err(io_at(&path)(e));
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(config: &str) -> [Vec<String>; 4] {
        let found = movement_keys(config);
        MoveDirection::ALL.map(|d| found.keys(d).to_vec())
    }

    fn v(keys: &[&str]) -> Vec<String> {
        keys.iter().map(|k| (*k).to_owned()).collect()
    }

    #[test]
    fn wasd_from_a_stock_config() {
        let config = "unbindall\nbind \"a\" \"+moveleft\"\nbind \"d\" \"+moveright\"\n\
                      bind \"s\" \"+back\"\nbind \"w\" \"+forward\"\nbind \"SPACE\" \"+jump\"\n";
        assert_eq!(keys(config), [v(&["w"]), v(&["s"]), v(&["a"]), v(&["d"])]);
    }

    #[test]
    fn esdf_keys_are_read() {
        let config = "bind \"e\" \"+forward\"\nbind \"d\" \"+back\"\n\
                      bind \"s\" \"+moveleft\"\nbind \"f\" \"+moveright\"\n";
        assert_eq!(keys(config), [v(&["e"]), v(&["d"]), v(&["s"]), v(&["f"])]);
    }

    #[test]
    fn arrows_and_wasd_both_count() {
        let config = "bind \"w\" \"+forward\"\nbind \"UPARROW\" \"+forward\"\n\
                      bind \"s\" \"+back\"\nbind \"DOWNARROW\" \"+back\"\n\
                      bind \"a\" \"+moveleft\"\nbind \"LEFTARROW\" \"+moveleft\"\n\
                      bind \"d\" \"+moveright\"\nbind \"RIGHTARROW\" \"+moveright\"\n";
        assert_eq!(
            keys(config),
            [
                v(&["w", "UPARROW"]),
                v(&["s", "DOWNARROW"]),
                v(&["a", "LEFTARROW"]),
                v(&["d", "RIGHTARROW"])
            ]
        );
    }

    #[test]
    fn keys_already_on_the_addon_count() {
        let config =
            "bind \"w\" \"+mf\"\nbind \"s\" \"+mb\"\nbind \"a\" \"+ml\"\nbind \"d\" \"+mr\"\n";
        assert_eq!(keys(config), [v(&["w"]), v(&["s"]), v(&["a"]), v(&["d"])]);
    }

    #[test]
    fn a_direction_without_a_key_is_missing() {
        let found = movement_keys("bind \"w\" \"+forward\"\nbind \"a\" \"+moveleft\"\n");
        assert_eq!(found.missing(), [MoveDirection::Back, MoveDirection::Right]);
        assert!(movement_keys("").missing().len() == 4);
    }

    #[test]
    fn crlf_lines_and_unquoted_binds_are_read() {
        let config = "unbindall\r\nbind \"w\" \"+forward\"\r\nbind s +back\r\n";
        assert_eq!(keys(config)[..2], [v(&["w"]), v(&["s"])]);
    }

    #[test]
    fn commented_binds_are_ignored() {
        let config = "// bind \"w\" \"+forward\"\n  //bind \"s\" \"+back\"\n\
                      bind \"a\" \"+moveleft\" // strafe\n";
        assert_eq!(keys(config), [v(&[]), v(&[]), v(&["a"]), v(&[])]);
    }

    #[test]
    fn combined_commands_and_aliases_are_left_alone() {
        let config = "bind \"w\" \"+forward;+jump\"\nbind s +back; echo hi\n\
                      bind \"a\" \"+myleft\"\nbind \"d\" \"+moveright\"\n";
        assert_eq!(keys(config), [v(&[]), v(&[]), v(&[]), v(&["d"])]);
    }

    #[test]
    fn a_key_bound_again_later_keeps_only_its_last_bind() {
        let config = "bind \"w\" \"+forward\"\nbind \"w\" \"+jump\"\nbind \"s\" \"+back\"\n\
                      unbind \"s\"\nbind \"a\" \"+moveleft\"\nunbindall\nbind \"d\" \"+moveright\"\n";
        assert_eq!(keys(config), [v(&[]), v(&[]), v(&[]), v(&["d"])]);
    }

    #[test]
    fn unsafe_keys_are_rejected() {
        let config = "bind w;x +forward\nbind \"s\" \"+back\"\n";
        assert_eq!(keys(config)[..2], [v(&[]), v(&["s"])]);
    }

    #[test]
    fn restore_rewrites_only_the_addon_binds() {
        let config = "unbindall\r\nbind \"w\" \"+mf\"\r\nbind \"UPARROW\" \"+mf\"\r\n\
                      bind \"s\" \"+mb\"\r\nbind a +ml\r\nbind \"d\" \"+MR\" // right\r\n\
                      bind \"x\" \"+mf;+jump\"\r\n// bind \"q\" \"+mf\"\r\nbind \"e\" \"+use\"\r\n";
        let want = "unbindall\r\nbind \"w\" \"+forward\"\r\nbind \"UPARROW\" \"+forward\"\r\n\
                    bind \"s\" \"+back\"\r\nbind a +moveleft\r\nbind \"d\" \"+moveright\" // right\r\n\
                    bind \"x\" \"+mf;+jump\"\r\n// bind \"q\" \"+mf\"\r\nbind \"e\" \"+use\"\r\n";
        assert_eq!(
            restore_stock_binds(config.as_bytes()).as_deref(),
            Some(want.as_bytes())
        );
    }

    #[test]
    fn restore_changes_nothing_without_addon_binds() {
        assert_eq!(restore_stock_binds(b"bind \"w\" \"+forward\"\n"), None);
        assert_eq!(restore_stock_binds(b""), None);
    }
}
