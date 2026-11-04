//! Text files the installers write into `tf/custom/`: crosshair VMTs, weapon scripts
//! and mastercomfig's `addons.cfg`. Ported from v5's `services/vdf.rs` and `services/tf2.rs`.

use std::borrow::Cow;
use std::fmt::Write as _;

use crate::AddonName;

pub(crate) const ADDON_PREFIX: &str = "mastercomfig-addon-";

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

/// A weapon script that points the weapon's crosshair at `texture_name(crosshair)`. TF2 only reads
/// the crosshair when it is nested under `TextureData`.
pub(crate) fn weapon_script(crosshair: &str) -> String {
    let crosshair = texture_name(crosshair);
    format!(
        "\"WeaponData\"\n{{\n\t\"TextureData\"\n\t{{\n\t\t\"crosshair\"\n\t\t{{\n\t\t\t\"file\"\t\t\"vgui/replay/thumbnails/{crosshair}\"\n\t\t\t\"x\"\t\t\"0\"\n\t\t\t\"y\"\t\t\"0\"\n\t\t\t\"width\"\t\t\"64\"\n\t\t\t\"height\"\t\"64\"\n\t\t}}\n\t}}\n}}"
    )
}

/// Whether the text of a weapon script points at `texture_name(crosshair)`. The closing quote
/// keeps a longer name that merely starts the same from matching.
pub(crate) fn names_texture(script: &str, crosshair: &str) -> bool {
    let crosshair = texture_name(crosshair);
    script.contains(&format!("\"vgui/replay/thumbnails/{crosshair}\""))
}

/// `exec addons/<name>.cfg` lines for each file name `AddonName` accepts, sorted;
/// None when there are none.
pub(crate) fn addons_cfg(file_names: &[&str]) -> Option<String> {
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

    #[test]
    fn weapon_script_matches_v5() {
        assert_eq!(
            weapon_script("my-cross-1a2b3c4d"),
            "\"WeaponData\"\n{\n\t\"TextureData\"\n\t{\n\t\t\"crosshair\"\n\t\t{\n\t\t\t\"file\"\t\t\"vgui/replay/thumbnails/my-cross-1a2b3c4d\"\n\t\t\t\"x\"\t\t\"0\"\n\t\t\t\"y\"\t\t\"0\"\n\t\t\t\"width\"\t\t\"64\"\n\t\t\t\"height\"\t\"64\"\n\t\t}\n\t}\n}"
        );
    }

    #[test]
    fn a_quote_in_a_crosshair_name_never_reaches_the_text() {
        assert_eq!(vmt("q\"t-1a2b3c4d"), vmt("q-t-1a2b3c4d"));
        assert_eq!(
            weapon_script("q\"t-1a2b3c4d"),
            weapon_script("q-t-1a2b3c4d")
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
            addons_cfg(&names).as_deref(),
            Some("exec addons/a.cfg\nexec addons/b.cfg\n")
        );
    }

    #[test]
    fn addons_cfg_is_none_without_addons() {
        assert_eq!(addons_cfg(&[]), None);
        assert_eq!(addons_cfg(&["tf2tools-hitsound.vpk"]), None);
    }

    #[test]
    fn addons_cfg_skips_names_the_addon_rule_rejects() {
        let names = [
            "mastercomfig-addon-.vpk",
            "mastercomfig-addon-a..b.vpk",
            "mastercomfig-addon-a.vpk",
        ];
        assert_eq!(addons_cfg(&names).as_deref(), Some("exec addons/a.cfg\n"));
        assert_eq!(addons_cfg(&["mastercomfig-addon-.vpk"]), None);
    }
}
