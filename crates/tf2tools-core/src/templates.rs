//! Text files the installers write into `tf/custom/`: crosshair VMTs, weapon scripts
//! and mastercomfig's `addons.cfg`. Ported from v5's `services/vdf.rs` and `services/tf2.rs`.

use std::borrow::Cow;
use std::fmt::Write as _;

use crate::AddonName;
use crate::library::{MoveDirection, MovementKeys, NULL_MOVEMENT};

pub(crate) const ADDON_PREFIX: &str = "mastercomfig-addon-";
/// mastercomfig's base VPK, both as a release asset and in `tf/custom`.
pub(crate) const BASE_VPK: &str = "mastercomfig-base.vpk";
/// The name every override VPK starts with; the addon list leaves these out.
pub(crate) const OVERRIDE_PREFIX: &str = "mastercomfig-addon-override-";

/// The file name of mastercomfig's override VPK in 0-based `slot`; slot 0 is `...-override-1.vpk`.
pub(crate) fn override_vpk(slot: usize) -> String {
    format!("{OVERRIDE_PREFIX}{}.vpk", slot + 1)
}

/// The 0-based slot of an exact override VPK name, for the three slots mastercomfig ships.
pub(crate) fn override_slot(file: &str) -> Option<usize> {
    (0..3).find(|&slot| file == override_vpk(slot))
}

/// The name a crosshair's texture is installed and referenced under: `crosshair` with every
/// `"` and control character turned into `-`, so it can sit inside a quoted VMT string.
pub(crate) fn texture_name(crosshair: &str) -> Cow<'_, str> {
    let unsafe_char = |c: char| c == '"' || c.is_control();
    if crosshair.contains(unsafe_char) {
        Cow::Owned(crosshair.replace(unsafe_char, "-"))
    } else {
        Cow::Borrowed(crosshair)
    }
}

/// The VMT for a crosshair texture at `vgui/replay/thumbnails/<texture_name(crosshair)>`.
pub(crate) fn vmt(crosshair: &str) -> String {
    let crosshair = texture_name(crosshair);
    format!(
        "\"UnlitGeneric\"\n{{\n\t\"$translucent\" \"1\"\n\t\"$basetexture\" \"vgui/replay/thumbnails/{crosshair}\"\n\t\"$vertexcolor\" \"1\"\n\t\"$no_fullbright\" \"1\"\n\t\"$ignorez\" \"1\"\n}}"
    )
}

/// The game's own weapon script `stock` with its `TextureData` crosshair block pointed at
/// `texture_name(crosshair)`; every other byte is kept, since TF2 takes a loose script as the
/// weapon's whole definition. None when `stock` has no such block or its braces do not balance.
pub(crate) fn weapon_script(stock: &[u8], crosshair: &str) -> Option<Vec<u8>> {
    let (start, end) = crosshair_block(stock)?;
    let crosshair = texture_name(crosshair);
    let block = format!(
        "{{\n\t\t\t\"file\"\t\t\"vgui/replay/thumbnails/{crosshair}\"\n\t\t\t\"x\"\t\t\"0\"\n\t\t\t\"y\"\t\t\"0\"\n\t\t\t\"width\"\t\t\"64\"\n\t\t\t\"height\"\t\"64\"\n\t\t}}"
    );
    let mut out = Vec::with_capacity(stock.len() + block.len());
    out.extend_from_slice(&stock[..start]);
    out.extend_from_slice(block.as_bytes());
    out.extend_from_slice(&stock[end..]);
    Some(out)
}

/// The byte span, braces included, of the first `crosshair` block directly under the root's
/// `TextureData`. Walks KeyValues tokens (quoted or bare, `//` comments skipped) and tracks the
/// key that opened each block; None when there is no such block or the braces do not balance.
fn crosshair_block(text: &[u8]) -> Option<(usize, usize)> {
    let mut keys: Vec<&[u8]> = Vec::new();
    let mut last: &[u8] = b"";
    let mut found: Option<(usize, usize)> = None;
    // The open brace of the crosshair block and the depth it sits at, while inside it.
    let mut open: Option<(usize, usize)> = None;
    let mut i = 0;
    while i < text.len() {
        match text[i] {
            b if b.is_ascii_whitespace() => i += 1,
            b'/' if text.get(i + 1) == Some(&b'/') => {
                while i < text.len() && text[i] != b'\n' {
                    i += 1;
                }
            }
            b'{' => {
                let at_crosshair = found.is_none()
                    && open.is_none()
                    && keys.len() == 2
                    && keys[1].eq_ignore_ascii_case(b"texturedata")
                    && last.eq_ignore_ascii_case(b"crosshair");
                if at_crosshair {
                    open = Some((i, keys.len()));
                }
                keys.push(last);
                last = b"";
                i += 1;
            }
            b'}' => {
                keys.pop()?;
                if let Some((start, depth)) = open
                    && keys.len() == depth
                {
                    found = Some((start, i + 1));
                    open = None;
                }
                last = b"";
                i += 1;
            }
            b'"' => {
                let len = text[i + 1..].iter().position(|&b| b == b'"')?;
                last = &text[i + 1..i + 1 + len];
                i += len + 2;
            }
            _ => {
                let len = text[i..]
                    .iter()
                    .position(|&b| b.is_ascii_whitespace() || matches!(b, b'{' | b'}' | b'"'))
                    .unwrap_or(text.len() - i);
                last = &text[i..i + len];
                i += len;
            }
        }
    }
    if keys.is_empty() { found } else { None }
}

/// Whether the text of a weapon script points at `texture_name(crosshair)`. The closing quote
/// keeps a longer name that merely starts the same from matching.
pub(crate) fn names_texture(script: &str, crosshair: &str) -> bool {
    let crosshair = texture_name(crosshair);
    script.contains(&format!("\"vgui/replay/thumbnails/{crosshair}\""))
}

/// `exec addons/<name>.cfg` lines for each file name `AddonName` accepts, sorted;
/// None when there are none. Null-Canceling Movement's exec is followed by binds of
/// `movement`'s keys to the addon's commands.
pub(crate) fn addons_cfg(file_names: &[&str], movement: &MovementKeys) -> Option<String> {
    // The AddonName rule decides what is an addon, so a name the library refuses never
    // reaches the cfg; sort by full file name as v5 did.
    let mut addons: Vec<AddonName> = file_names
        .iter()
        .filter_map(|name| AddonName::new(*name))
        .collect();
    if addons.is_empty() {
        return None;
    }
    addons.sort_unstable_by(|a, b| a.as_str().cmp(b.as_str()));
    let mut content = String::new();
    for addon in &addons {
        let _ = writeln!(content, "exec addons/{}.cfg", addon.stem());
        // The addon only defines +mf/+mb/+ml/+mr; these binds put the player's own keys on
        // them, after config.cfg runs and before the player's overrides can change them.
        if addon.as_str() == NULL_MOVEMENT && !movement.is_empty() {
            content.push_str("// TF2Tools: movement keys for Null-Canceling Movement\n");
            for direction in MoveDirection::ALL {
                for key in movement.keys(direction) {
                    let _ = writeln!(content, "bind \"{key}\" \"{}\"", direction.null_command());
                }
            }
        }
    }
    Some(content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vmt_matches_v5() {
        assert_eq!(
            vmt("cpma_9"),
            "\"UnlitGeneric\"\n{\n\t\"$translucent\" \"1\"\n\t\"$basetexture\" \"vgui/replay/thumbnails/cpma_9\"\n\t\"$vertexcolor\" \"1\"\n\t\"$no_fullbright\" \"1\"\n\t\"$ignorez\" \"1\"\n}"
        );
    }

    // A stock script shaped like TF2's: CRLF lines, comments, a bare TextureData key, a
    // decoy crosshair outside TextureData and blocks either side of the one replaced.
    const STOCK: &[u8] = b"WeaponData\r\n{\r\n\t// \"crosshair\" { in a comment }\r\n\t\"crosshair\"\t\"keep\"\r\n\t\"Damage\"\t\"6\"\r\n\tTextureData\r\n\t{\r\n\t\t\"ammo\"\r\n\t\t{\r\n\t\t\t\"file\"\t\"sprites/a_icons1\"\r\n\t\t}\r\n\t\t\"crosshair\"\r\n\t\t{\r\n\t\t\t\t\"file\"\t\t\"sprites/crosshairs\"\r\n\t\t\t\t\"x\"\t\t\"0\"\r\n\t\t\t\t\"y\"\t\t\"0\"\r\n\t\t\t\t\"width\"\t\t\"32\"\r\n\t\t\t\t\"height\"\t\"32\"\r\n\t\t}\r\n\t\t\"autoaim\"\r\n\t\t{\r\n\t\t\t\"file\"\t\"sprites/crosshairs\"\r\n\t\t}\r\n\t}\r\n}\r\n";

    const OURS: &str = "{\n\t\t\t\"file\"\t\t\"vgui/replay/thumbnails/my-cross-1a2b3c4d\"\n\t\t\t\"x\"\t\t\"0\"\n\t\t\t\"y\"\t\t\"0\"\n\t\t\t\"width\"\t\t\"64\"\n\t\t\t\"height\"\t\"64\"\n\t\t}";

    #[test]
    fn weapon_script_replaces_only_the_texture_data_crosshair_block() {
        let stock = String::from_utf8(STOCK.to_vec()).unwrap();
        let start = stock.find("\"crosshair\"\r\n\t\t{").unwrap() + "\"crosshair\"\r\n\t\t".len();
        let end = stock[start..].find('}').unwrap() + start + 1;
        let expected = format!("{}{OURS}{}", &stock[..start], &stock[end..]);
        let patched = weapon_script(STOCK, "my-cross-1a2b3c4d").unwrap();
        assert_eq!(String::from_utf8(patched).unwrap(), expected);
    }

    #[test]
    fn weapon_script_keeps_bytes_that_are_not_utf8() {
        let mut stock = b"// caf\xe9\n".to_vec();
        stock.extend_from_slice(STOCK);
        let patched = weapon_script(&stock, "x").unwrap();
        assert!(patched.starts_with(b"// caf\xe9\nWeaponData"));
    }

    #[test]
    fn a_patched_script_names_its_texture() {
        let patched = weapon_script(STOCK, "my-cross-1a2b3c4d").unwrap();
        let text = String::from_utf8_lossy(&patched);
        assert!(names_texture(&text, "my-cross-1a2b3c4d"));
        assert!(!names_texture(&text, "my-cross"));
    }

    #[test]
    fn weapon_script_is_none_without_a_texture_data_crosshair_block() {
        assert_eq!(weapon_script(b"", "x"), None);
        assert_eq!(
            weapon_script(b"WeaponData { \"crosshair\" { \"file\" \"a\" } }", "x"),
            None
        );
        assert_eq!(
            weapon_script(b"WeaponData { TextureData { \"ammo\" { } } }", "x"),
            None
        );
        assert_eq!(
            weapon_script(
                b"WeaponData { TextureData { \"crosshair\" \"flat\" } }",
                "x"
            ),
            None
        );
    }

    #[test]
    fn weapon_script_is_none_when_braces_do_not_balance() {
        assert_eq!(
            weapon_script(
                b"WeaponData { TextureData { \"crosshair\" { \"file\" \"a\" ",
                "x"
            ),
            None
        );
        assert_eq!(
            weapon_script(b"WeaponData { TextureData { \"crosshair\" { } }", "x"),
            None
        );
    }

    #[test]
    fn a_quote_in_a_crosshair_name_never_reaches_the_text() {
        assert_eq!(vmt("q\"t-1a2b3c4d"), vmt("q-t-1a2b3c4d"));
        assert_eq!(
            weapon_script(STOCK, "q\"t-1a2b3c4d"),
            weapon_script(STOCK, "q-t-1a2b3c4d")
        );
        assert_eq!(vmt("a\nb"), vmt("a-b"));
    }

    #[test]
    fn addons_cfg_lists_only_addon_vpks_sorted() {
        let names = [
            "mastercomfig-addon-b.vpk",
            "readme.txt",
            "mastercomfig-addon-a.VPK",
            "other.vpk",
            "mastercomfig-addon-c.vpk.bak",
        ];
        assert_eq!(
            addons_cfg(&names, &none()).as_deref(),
            Some("exec addons/a.cfg\nexec addons/b.cfg\n")
        );
    }

    #[test]
    fn addons_cfg_is_none_without_addons() {
        assert_eq!(addons_cfg(&[], &none()), None);
        assert_eq!(addons_cfg(&["tf2tools-hitsound.vpk"], &none()), None);
    }

    #[test]
    fn addons_cfg_skips_names_the_addon_rule_rejects() {
        let names = [
            "mastercomfig-addon-.vpk",
            "mastercomfig-addon-a..b.vpk",
            "mastercomfig-addon-a.vpk",
        ];
        assert_eq!(
            addons_cfg(&names, &none()).as_deref(),
            Some("exec addons/a.cfg\n")
        );
        assert_eq!(addons_cfg(&["mastercomfig-addon-.vpk"], &none()), None);
    }

    fn none() -> MovementKeys {
        MovementKeys::default()
    }

    #[test]
    fn addons_cfg_binds_movement_keys_after_the_null_canceling_exec() {
        let movement = crate::library::movement_keys(
            "bind \"w\" \"+forward\"\nbind \"UPARROW\" \"+forward\"\nbind \"s\" \"+back\"\n\
             bind \"a\" \"+moveleft\"\nbind \"d\" \"+moveright\"\n",
        );
        let names = [
            "mastercomfig-addon-flat-mouse.vpk",
            "mastercomfig-addon-null-canceling-movement.vpk",
            "mastercomfig-addon-no-tutorial.vpk",
        ];
        assert_eq!(
            addons_cfg(&names, &movement).as_deref(),
            Some(
                "exec addons/flat-mouse.cfg\n\
                 exec addons/no-tutorial.cfg\n\
                 exec addons/null-canceling-movement.cfg\n\
                 // TF2Tools: movement keys for Null-Canceling Movement\n\
                 bind \"w\" \"+mf\"\n\
                 bind \"UPARROW\" \"+mf\"\n\
                 bind \"s\" \"+mb\"\n\
                 bind \"a\" \"+ml\"\n\
                 bind \"d\" \"+mr\"\n"
            )
        );
    }

    #[test]
    fn addons_cfg_has_no_binds_without_the_addon_or_without_keys() {
        let movement = crate::library::movement_keys("bind \"w\" \"+forward\"\n");
        assert_eq!(
            addons_cfg(&["mastercomfig-addon-flat-mouse.vpk"], &movement).as_deref(),
            Some("exec addons/flat-mouse.cfg\n")
        );
        assert_eq!(
            addons_cfg(&["mastercomfig-addon-null-canceling-movement.vpk"], &none()).as_deref(),
            Some("exec addons/null-canceling-movement.cfg\n")
        );
    }
}
