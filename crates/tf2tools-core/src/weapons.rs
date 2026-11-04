//! The crosshair-assignable weapon scripts, ported from v5's `src/lib/data/weapons.ts`.

/// A TF2 class, in v5's `TF2_CLASSES` order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum TfClass {
    /// Scout.
    Scout,
    /// Soldier.
    Soldier,
    /// Pyro.
    Pyro,
    /// Demoman.
    Demoman,
    /// Heavy.
    Heavy,
    /// Engineer.
    Engineer,
    /// Medic.
    Medic,
    /// Sniper.
    Sniper,
    /// Spy.
    Spy,
}

impl TfClass {
    /// Every class, in v5's display order.
    pub const ALL: [TfClass; 9] = [
        TfClass::Scout,
        TfClass::Soldier,
        TfClass::Pyro,
        TfClass::Demoman,
        TfClass::Heavy,
        TfClass::Engineer,
        TfClass::Medic,
        TfClass::Sniper,
        TfClass::Spy,
    ];

    /// The class's display name, such as `"Scout"`.
    pub fn name(self) -> &'static str {
        match self {
            TfClass::Scout => "Scout",
            TfClass::Soldier => "Soldier",
            TfClass::Pyro => "Pyro",
            TfClass::Demoman => "Demoman",
            TfClass::Heavy => "Heavy",
            TfClass::Engineer => "Engineer",
            TfClass::Medic => "Medic",
            TfClass::Sniper => "Sniper",
            TfClass::Spy => "Spy",
        }
    }
}

/// The loadout slot a weapon script belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum WeaponSlot {
    /// Primary slot.
    Primary,
    /// Secondary slot.
    Secondary,
    /// Melee slot.
    Melee,
}

impl WeaponSlot {
    /// The slot's display name, such as `"Primary"`.
    pub fn name(self) -> &'static str {
        match self {
            WeaponSlot::Primary => "Primary",
            WeaponSlot::Secondary => "Secondary",
            WeaponSlot::Melee => "Melee",
        }
    }
}

/// One crosshair-assignable weapon script and the in-game weapons it covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct Weapon {
    class: TfClass,
    script_name: &'static str,
    display_name: &'static str,
    slot: WeaponSlot,
    weapons_affected: &'static [&'static str],
}

impl Weapon {
    /// The class that uses this weapon script.
    pub fn class(&self) -> TfClass {
        self.class
    }

    /// The script file stem under `scripts/`, such as `"tf_weapon_bat"`.
    pub fn script_name(&self) -> &'static str {
        self.script_name
    }

    /// The name v5 shows for the script.
    pub fn display_name(&self) -> &'static str {
        self.display_name
    }

    /// The loadout slot.
    pub fn slot(&self) -> WeaponSlot {
        self.slot
    }

    /// The in-game weapons that share this script, in v5's order.
    pub fn weapons_affected(&self) -> &'static [&'static str] {
        self.weapons_affected
    }
}

/// Every crosshair-assignable weapon script, in v5's `TF2_WEAPONS` order.
pub fn weapons() -> &'static [Weapon] {
    &WEAPONS
}

const fn w(
    class: TfClass,
    script_name: &'static str,
    display_name: &'static str,
    slot: WeaponSlot,
    weapons_affected: &'static [&'static str],
) -> Weapon {
    Weapon {
        class,
        script_name,
        display_name,
        slot,
        weapons_affected,
    }
}

// Generated from weapons.ts by a one-off script; the fingerprint test in
// tests/weapons.rs pins every field.
static WEAPONS: [Weapon; 64] = [
    w(
        TfClass::Scout,
        "tf_weapon_pep_brawler_blaster",
        "Baby Face's Blaster",
        WeaponSlot::Primary,
        &["Baby Face's Blaster"],
    ),
    w(
        TfClass::Scout,
        "tf_weapon_lunchbox_drink",
        "Bonk! Atomic Punch",
        WeaponSlot::Secondary,
        &["Bonk! Atomic Punch", "Crit-a-Cola"],
    ),
    w(
        TfClass::Scout,
        "tf_weapon_bat_fish",
        "Holy Mackerel",
        WeaponSlot::Melee,
        &["The Holy Mackerel", "Unarmed Combat"],
    ),
    w(
        TfClass::Scout,
        "tf_weapon_jar_milk",
        "Mad Milk",
        WeaponSlot::Secondary,
        &["Mad Milk", "Mutated Milk"],
    ),
    w(
        TfClass::Scout,
        "tf_weapon_bat",
        "Most Melee",
        WeaponSlot::Melee,
        &[
            "Bat",
            "The Candy Cane",
            "The Boston Basher",
            "Sun-on-a-Stick",
            "The Fan O'War",
            "The Atomizer",
            "Three-Rune Blade",
            "Batsaber",
            "Frying Pan",
            "Saxxy",
            "The Conscientious Objector",
            "The Freedom Staff",
            "The Bat Outta Hell",
            "The Memory Maker",
            "The Ham Shank",
            "Gold Frying Pan",
            "The Necro Smasher",
            "The Crossing Guard",
            "Prinny Machete",
        ],
    ),
    w(
        TfClass::Scout,
        "tf_weapon_bat_wood",
        "Sandman",
        WeaponSlot::Melee,
        &["The Sandman"],
    ),
    w(
        TfClass::Scout,
        "tf_weapon_scattergun",
        "Scatterguns",
        WeaponSlot::Primary,
        &["Scattergun", "Force-A-Nature", "The Back Scatter"],
    ),
    w(
        TfClass::Scout,
        "tf_weapon_handgun_scout_primary",
        "Shortstop",
        WeaponSlot::Primary,
        &["The Shortstop"],
    ),
    w(
        TfClass::Scout,
        "tf_weapon_soda_popper",
        "Soda Popper",
        WeaponSlot::Primary,
        &["The Soda Popper"],
    ),
    w(
        TfClass::Scout,
        "tf_weapon_pistol_scout",
        "Stock Pistols",
        WeaponSlot::Secondary,
        &["Pistol", "Lugermorph", "The C.A.P.P.E.R."],
    ),
    w(
        TfClass::Scout,
        "tf_weapon_handgun_scout_secondary",
        "Winger/Pocket Pistol",
        WeaponSlot::Secondary,
        &["The Winger", "Pocket Pistol"],
    ),
    w(
        TfClass::Scout,
        "tf_weapon_bat_giftwrap",
        "Wrap Assassin",
        WeaponSlot::Melee,
        &["The Wrap Assassin"],
    ),
    w(
        TfClass::Scout,
        "tf_weapon_cleaver",
        "Flying Guillotine",
        WeaponSlot::Secondary,
        &["Flying Guillotine"],
    ),
    w(
        TfClass::Soldier,
        "tf_weapon_rocketlauncher_airstrike",
        "Air Strike",
        WeaponSlot::Primary,
        &["The Air Strike"],
    ),
    w(
        TfClass::Soldier,
        "tf_weapon_buff_item",
        "Banners",
        WeaponSlot::Secondary,
        &["The Buff Banner", "The Battalion's Backup", "The Concheror"],
    ),
    w(
        TfClass::Soldier,
        "tf_weapon_particle_cannon",
        "Cow Mangler 5000",
        WeaponSlot::Primary,
        &["Cow Mangler 5000"],
    ),
    w(
        TfClass::Soldier,
        "tf_weapon_rocketlauncher_directhit",
        "Direct Hit",
        WeaponSlot::Primary,
        &["The Direct Hit"],
    ),
    w(
        TfClass::Soldier,
        "tf_weapon_shovel",
        "Melee",
        WeaponSlot::Melee,
        &[
            "Shovel",
            "The Equalizer",
            "The Pain Train",
            "Frying Pan",
            "The Half-Zatoichi",
            "The Market Gardener",
            "The Disciplinary Action",
            "The Conscientious Objector",
            "The Freedom Staff",
            "The Bat Outta Hell",
            "The Memory Maker",
            "Gold Frying Pan",
            "The Necro Smasher",
            "The Crossing Guard",
            "Prinny Machete",
        ],
    ),
    w(
        TfClass::Soldier,
        "tf_weapon_raygun",
        "Righteous Bison",
        WeaponSlot::Secondary,
        &["The Righteous Bison"],
    ),
    w(
        TfClass::Soldier,
        "tf_weapon_rocketlauncher",
        "Rocket Launcher",
        WeaponSlot::Primary,
        &[
            "Rocket Launcher",
            "The Black Box",
            "Rocket Jumper",
            "The Liberty Launcher",
            "The Original",
            "The Beggar's Bazooka",
        ],
    ),
    w(
        TfClass::Soldier,
        "tf_weapon_shotgun_soldier",
        "Shotguns",
        WeaponSlot::Secondary,
        &["Soldier's Shotgun", "The Reserve Shooter", "Panic Attack"],
    ),
    w(
        TfClass::Pyro,
        "tf_weapon_rocketlauncher_fireball",
        "Dragon's Fury",
        WeaponSlot::Primary,
        &["Dragon's Fury"],
    ),
    w(
        TfClass::Pyro,
        "tf_weapon_flamethrower",
        "Flamethrowers",
        WeaponSlot::Primary,
        &[
            "Flame Thrower",
            "The Backburner",
            "The Phlogistinator",
            "The Rainblower",
            "Nostromo Napalmer",
        ],
    ),
    w(
        TfClass::Pyro,
        "tf_weapon_flaregun",
        "Flare Guns",
        WeaponSlot::Secondary,
        &["The Flare Gun", "The Detonator", "The Scorch Shot"],
    ),
    w(
        TfClass::Pyro,
        "tf_weapon_slap",
        "Hot Hand",
        WeaponSlot::Melee,
        &["Hot Hand"],
    ),
    w(
        TfClass::Pyro,
        "tf_weapon_flaregun_revenge",
        "Manmelter",
        WeaponSlot::Secondary,
        &["Manmelter"],
    ),
    w(
        TfClass::Pyro,
        "tf_weapon_fireaxe",
        "Most Melee",
        WeaponSlot::Melee,
        &[
            "Fire Axe",
            "The Axtinguisher",
            "Homewrecker",
            "The Powerjack",
            "Frying Pan",
            "The Back Scratcher",
            "Sharpened Volcano Fragment",
            "Saxxy",
            "The Postal Pummeler",
            "The Maul",
            "The Conscientious Objector",
            "The Third Degree",
            "The Lollichop",
            "The Freedom Staff",
            "The Bat Outta Hell",
            "The Memory Maker",
            "The Ham Shank",
            "Gold Frying Pan",
            "The Necro Smasher",
            "The Crossing Guard",
            "Prinny Machete",
        ],
    ),
    w(
        TfClass::Pyro,
        "tf_weapon_shotgun_pyro",
        "Shotguns",
        WeaponSlot::Secondary,
        &[
            "Pyro's Shotgun",
            "The Reserve Shooter",
            "Panic Attack",
            "Thermal Thruster",
            "Gas Passer",
        ],
    ),
    w(
        TfClass::Pyro,
        "tf_weapon_rocketpack",
        "Thermal Thruster",
        WeaponSlot::Secondary,
        &["Thermal Thruster"],
    ),
    w(
        TfClass::Demoman,
        "tf_weapon_grenadelauncher",
        "Grenade Launchers",
        WeaponSlot::Primary,
        &["Grenade Launcher", "The Loch-n-Load", "The Iron Bomber"],
    ),
    w(
        TfClass::Demoman,
        "tf_weapon_cannon",
        "Loose Cannon",
        WeaponSlot::Primary,
        &["The Loose Cannon"],
    ),
    w(
        TfClass::Demoman,
        "tf_weapon_bottle",
        "Most Melee",
        WeaponSlot::Melee,
        &[
            "Bottle",
            "The Scottish Handshake",
            "Frying Pan",
            "Saxxy",
            "The Conscientious Objector",
            "The Freedom Staff",
            "The Bat Outta Hell",
            "The Memory Maker",
            "The Ham Shank",
            "The Necro Smasher",
            "The Crossing Guard",
            "Prinny Machete",
        ],
    ),
    w(
        TfClass::Demoman,
        "tf_weapon_katana",
        "Half-Zatoichi",
        WeaponSlot::Melee,
        &["The Half-Zatoichi"],
    ),
    w(
        TfClass::Demoman,
        "tf_weapon_pipebomblauncher",
        "Sticky Launchers",
        WeaponSlot::Secondary,
        &[
            "Stickybomb Launcher",
            "The Scottish Resistance",
            "Sticky Jumper",
            "The Quickiebomb Launcher",
        ],
    ),
    w(
        TfClass::Demoman,
        "tf_weapon_sword",
        "Swords",
        WeaponSlot::Melee,
        &[
            "The Eyelander",
            "The Scotsman's Skullcutter",
            "Horseless Headless Horsemann's Headtaker",
            "The Claidheamh Mor",
            "The Persian Persuader",
            "Nessie's Nine Iron",
        ],
    ),
    w(
        TfClass::Demoman,
        "tf_weapon_stickbomb",
        "Ullapool Caber",
        WeaponSlot::Melee,
        &["Ullapool Caber"],
    ),
    w(
        TfClass::Heavy,
        "tf_weapon_fists",
        "Most Melee",
        WeaponSlot::Melee,
        &[
            "Fists",
            "The Killing Gloves of Boxing",
            "Gloves of Running Urgently",
            "Frying Pan",
            "Warrior's Spirit",
            "Fists of Steel",
            "Saxxy",
            "The Eviction Notice",
            "The Conscientious Objector",
            "Apoco-Fists",
            "The Holiday Punch",
            "The Freedom Staff",
            "The Bat Outta Hell",
            "The Memory Maker",
            "The Ham Shank",
            "Gold Frying Pan",
            "The Bread Bite",
            "The Necro Smasher",
            "The Crossing Guard",
            "Gloves of Running Urgently MvM",
            "Prinny Machete",
        ],
    ),
    w(
        TfClass::Heavy,
        "tf_weapon_minigun",
        "Miniguns",
        WeaponSlot::Primary,
        &[
            "Minigun",
            "Natascha",
            "Iron Curtain",
            "The Brass Beast",
            "Tomislav",
            "The Huo-Long Heater",
        ],
    ),
    w(
        TfClass::Heavy,
        "tf_weapon_lunchbox",
        "Sandvich/Other food",
        WeaponSlot::Secondary,
        &[
            "Sandvich",
            "The Dalokohs Bar",
            "The Buffalo Steak Sandvich",
            "Fishcake",
            "Robo-Sandvich",
            "Second Banana",
        ],
    ),
    w(
        TfClass::Heavy,
        "tf_weapon_shotgun_hwg",
        "Shotguns",
        WeaponSlot::Secondary,
        &["Heavy's Shotgun", "The Family Business", "Panic Attack"],
    ),
    w(
        TfClass::Engineer,
        "tf_weapon_robot_arm",
        "Gunslinger",
        WeaponSlot::Melee,
        &["The Gunslinger"],
    ),
    w(
        TfClass::Engineer,
        "tf_weapon_wrench",
        "Most Melee",
        WeaponSlot::Melee,
        &[
            "Wrench",
            "The Southern Hospitality",
            "Golden Wrench",
            "The Jag",
            "Saxxy",
            "The Eureka Effect",
            "Gold Frying Pan",
            "The Necro Smasher",
            "Nutcracker",
            "Prinny Machete",
        ],
    ),
    w(
        TfClass::Engineer,
        "tf_weapon_pistol",
        "Pistols",
        WeaponSlot::Secondary,
        &["Engineer's Pistol", "Lugermorph", "The C.A.P.P.E.R"],
    ),
    w(
        TfClass::Engineer,
        "tf_weapon_shotgun_building_rescue",
        "Rescue Ranger",
        WeaponSlot::Primary,
        &["The Rescue Ranger"],
    ),
    w(
        TfClass::Engineer,
        "tf_weapon_mechanical_arm",
        "Short Circuit",
        WeaponSlot::Secondary,
        &["Short Circuit"],
    ),
    w(
        TfClass::Engineer,
        "tf_weapon_shotgun_primary",
        "Shotguns",
        WeaponSlot::Primary,
        &["Engineer's Shotgun", "The Widowmaker", "Panic Attack"],
    ),
    w(
        TfClass::Engineer,
        "tf_weapon_sentry_revenge",
        "The Frontier Justice",
        WeaponSlot::Primary,
        &["The Frontier Justice"],
    ),
    w(
        TfClass::Engineer,
        "tf_weapon_laser_pointer",
        "Wranglers",
        WeaponSlot::Secondary,
        &["The Wrangler", "The Giger Counter"],
    ),
    w(
        TfClass::Medic,
        "tf_weapon_crossbow",
        "Crusader's Crossbow",
        WeaponSlot::Primary,
        &["Crusader's Crossbow"],
    ),
    w(
        TfClass::Medic,
        "tf_weapon_medigun",
        "Mediguns",
        WeaponSlot::Secondary,
        &[
            "Medi Gun",
            "The Kritzkrieg",
            "The Quick-Fix",
            "The Vaccinator",
        ],
    ),
    w(
        TfClass::Medic,
        "tf_weapon_bonesaw",
        "Most Melee",
        WeaponSlot::Melee,
        &[
            "Bonesaw",
            "The Ubersaw",
            "The Vita-Saw",
            "Frying Pan",
            "Amputator",
            "The Solemn Vow",
            "Saxxy",
            "The Conscientious Objector",
            "The Freedom Staff",
            "The Bat Outta Hell",
            "The Memory Maker",
            "The Ham Shank",
            "Gold Frying Pan",
            "The Necro Smasher",
            "The Crossing Guard",
            "Prinny Machete",
        ],
    ),
    w(
        TfClass::Medic,
        "tf_weapon_syringegun_medic",
        "Syringe Guns",
        WeaponSlot::Primary,
        &["Syringe Gun", "The Blutsauger", "The Overdose"],
    ),
    w(
        TfClass::Sniper,
        "tf_weapon_compound_bow",
        "Bows",
        WeaponSlot::Primary,
        &["The Huntsman", "The Fortified Compound"],
    ),
    w(
        TfClass::Sniper,
        "tf_weapon_sniperrifle_classic",
        "Classic",
        WeaponSlot::Primary,
        &["The Classic"],
    ),
    w(
        TfClass::Sniper,
        "tf_weapon_charged_smg",
        "Cleaner's Carbine",
        WeaponSlot::Secondary,
        &["The Cleaner's Carbine"],
    ),
    w(
        TfClass::Sniper,
        "tf_weapon_jar",
        "Jarate",
        WeaponSlot::Secondary,
        &["Jarate", "The Self-Aware Beauty Mark"],
    ),
    w(
        TfClass::Sniper,
        "tf_weapon_club",
        "Most Melee",
        WeaponSlot::Melee,
        &[
            "Kukri",
            "The Tribalman's Shiv",
            "The Bushwacka",
            "Frying Pan",
            "The Shahanshah",
            "Saxxy",
            "The Conscientious Objector",
            "The Freedom Staff",
            "The Bat Outta Hell",
            "The Memory Maker",
            "The Ham Shank",
            "Gold Frying Pan",
            "The Necro Smasher",
            "The Crossing Guard",
            "Prinny Machete",
        ],
    ),
    w(
        TfClass::Sniper,
        "tf_weapon_sniperrifle",
        "Rifles",
        WeaponSlot::Primary,
        &[
            "Sniper Rifle",
            "The Sydney Sleeper",
            "The Bazaar Bargain",
            "The Machina",
            "The Hitman's Heatmaker",
            "The AWPer Hand",
            "Shooting Star",
        ],
    ),
    w(
        TfClass::Sniper,
        "tf_weapon_smg",
        "SMG",
        WeaponSlot::Secondary,
        &["SMG"],
    ),
    w(
        TfClass::Sniper,
        "tf_weapon_sniperrifle_decap",
        "The Bazaar Bargain",
        WeaponSlot::Primary,
        &["The Bazaar Bargain"],
    ),
    w(
        TfClass::Spy,
        "tf_weapon_sapper",
        "Other Sappers",
        WeaponSlot::Secondary,
        &["The Red-Tape Recorder", "The Ap-Sap", "The Snack Attack"],
    ),
    w(
        TfClass::Spy,
        "tf_weapon_knife",
        "Most Melee",
        WeaponSlot::Melee,
        &[
            "Knife",
            "Your Eternal Reward",
            "Conniver's Kunai",
            "Saxxy",
            "The Big Earner",
            "The Wanga Prick",
            "The Sharp Dresser",
            "The Spy-cicle",
            "The Black Rose",
            "Gold Frying Pan",
            "Prinny Machete",
        ],
    ),
    w(
        TfClass::Spy,
        "tf_weapon_revolver",
        "Revolvers",
        WeaponSlot::Primary,
        &[
            "Revolver",
            "The Ambassador",
            "Big Kill",
            "L'Etranger",
            "The Enforcer",
            "The Diamondback",
        ],
    ),
    w(
        TfClass::Spy,
        "tf_weapon_builder",
        "Stock Sapper",
        WeaponSlot::Secondary,
        &["Sapper"],
    ),
];
