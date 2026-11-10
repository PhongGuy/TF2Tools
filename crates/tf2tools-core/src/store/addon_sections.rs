//! The GameBanana sections the Addon Store browses, each a root category of TF2's Mod Index.

/// One Addon Store section: a GameBanana root category and the subcategories it hides.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct AddonSection {
    key: &'static str,
    title: &'static str,
    root_category: u32,
    exclude: &'static [u32],
    weapon_parents: &'static [u32],
}

impl AddonSection {
    /// Every section, in tab order. GUIs hides HUDs and Effects hides crosshairs,
    /// since those have their own stores.
    pub const ALL: [AddonSection; 5] = [
        Self::new("skins", "Skins", 7951, &[], SKIN_CLASSES),
        Self::new("effects", "Effects", 1090, &[1105], &[]),
        Self::new("guis", "GUIs", 1644, &[1649], &[]),
        Self::new("textures", "Textures", 587, &[], &[]),
        Self::new("gamefiles", "Game files", 2774, &[], &[]),
    ];

    /// GameBanana's TF2 maps, browsed on the Maps page; not an Addon Store tab, so not in `ALL`.
    pub const MAPS: AddonSection = Self::new("maps", "Maps", 5371, &[], &[]);

    const fn new(
        key: &'static str,
        title: &'static str,
        root_category: u32,
        exclude: &'static [u32],
        weapon_parents: &'static [u32],
    ) -> Self {
        Self {
            key,
            title,
            root_category,
            exclude,
            weapon_parents,
        }
    }

    /// The section whose stable key is `key`, from `ALL` or `MAPS`.
    pub fn by_key(key: &str) -> Option<AddonSection> {
        Self::ALL
            .into_iter()
            .chain([Self::MAPS])
            .find(|section| section.key == key)
    }

    /// The stable key used in settings and element ids.
    pub fn key(self) -> &'static str {
        self.key
    }

    /// The name shown to the player.
    pub fn title(self) -> &'static str {
        self.title
    }

    /// The GameBanana category the section lists.
    pub fn root_category(self) -> u32 {
        self.root_category
    }

    /// Subcategories whose items the section filters out.
    pub fn exclude(self) -> &'static [u32] {
        self.exclude
    }

    /// Skins: the 9 class categories, whose children feed the weapon Select. Empty elsewhere.
    pub fn weapon_parents(self) -> &'static [u32] {
        self.weapon_parents
    }
}

/// The class categories under Skins: Scout, Soldier, Pyro, Demoman, Heavy, Engineer,
/// Medic, Sniper, Spy.
const SKIN_CLASSES: &[u32] = &[7962, 7960, 7956, 7968, 7954, 7958, 7964, 7966, 7952];
