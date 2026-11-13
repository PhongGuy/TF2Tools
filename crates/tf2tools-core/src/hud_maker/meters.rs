//! Item meters: the game builds one meter per weapon type, and each loads the file its weapon
//! names, so the HUD tab previews one chosen file in the Item meter's place.

/// Which weapon's meter the HUD tab previews: each is one file the game's code loads for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[non_exhaustive]
pub enum MeterKind {
    /// The Sandman, Jarate, banners and most other items: the plain file.
    #[default]
    Plain,
    /// Bonk!, Crit-a-Cola and Mad Milk.
    Drink,
    /// The Soda Popper and Baby Face's Blaster.
    SodaPopper,
    /// The Flying Guillotine.
    Cleaver,
    /// The Heavy's minigun.
    Minigun,
    /// The Bazaar Bargain's heads.
    Heads,
    /// The Hitman's Heatmaker's focus.
    Focus,
    /// The Eyelander's heads and the Air Strike.
    Sword,
    /// The Cow Mangler 5000.
    CowMangler,
    /// The Righteous Bison.
    Bison,
    /// The Spy-cicle.
    Spycicle,
    /// The Spy's sapper.
    Sapper,
    /// The Diamondback.
    Revolver,
    /// The Frontier Justice and the Manmelter.
    Revenge,
    /// The Pomson 6000.
    Pomson,
    /// The Phlogistinator.
    Flamethrower,
    /// The Thermal Thruster.
    RocketPack,
    /// The Vita-Saw.
    Organs,
    /// A killstreak weapon's count.
    Killstreak,
    /// The bumper car's boost.
    Kart,
    /// The MvM canteen.
    Canteen,
}

impl MeterKind {
    /// Every kind, in the order the menu lists them.
    pub const ALL: [MeterKind; 21] = [
        MeterKind::Plain,
        MeterKind::Drink,
        MeterKind::SodaPopper,
        MeterKind::Cleaver,
        MeterKind::Minigun,
        MeterKind::Heads,
        MeterKind::Focus,
        MeterKind::Sword,
        MeterKind::CowMangler,
        MeterKind::Bison,
        MeterKind::Spycicle,
        MeterKind::Sapper,
        MeterKind::Revolver,
        MeterKind::Revenge,
        MeterKind::Pomson,
        MeterKind::Flamethrower,
        MeterKind::RocketPack,
        MeterKind::Organs,
        MeterKind::Killstreak,
        MeterKind::Kart,
        MeterKind::Canteen,
    ];

    /// The weapons that show this meter, as players name them.
    pub fn label(self) -> &'static str {
        match self {
            MeterKind::Plain => "Sandman, Jarate, banners and most items",
            MeterKind::Drink => "Bonk!, Crit-a-Cola and Mad Milk",
            MeterKind::SodaPopper => "Soda Popper and Baby Face's Blaster",
            MeterKind::Cleaver => "Flying Guillotine",
            MeterKind::Minigun => "Heavy's minigun",
            MeterKind::Heads => "Bazaar Bargain heads",
            MeterKind::Focus => "Hitman's Heatmaker focus",
            MeterKind::Sword => "Eyelander heads and Air Strike",
            MeterKind::CowMangler => "Cow Mangler",
            MeterKind::Bison => "Righteous Bison",
            MeterKind::Spycicle => "Spy-cicle",
            MeterKind::Sapper => "Sapper",
            MeterKind::Revolver => "Diamondback",
            MeterKind::Revenge => "Frontier Justice and Manmelter",
            MeterKind::Pomson => "Pomson 6000",
            MeterKind::Flamethrower => "Phlogistinator",
            MeterKind::RocketPack => "Thermal Thruster",
            MeterKind::Organs => "Vita-Saw",
            MeterKind::Killstreak => "Killstreak count",
            MeterKind::Kart => "Bumper car boost",
            MeterKind::Canteen => "MvM canteen",
        }
    }

    /// The lowercase game path the meter reads.
    pub fn file(self) -> &'static str {
        match self {
            MeterKind::Plain => "resource/ui/huditemeffectmeter.res",
            MeterKind::Drink => "resource/ui/huditemeffectmeter_scout.res",
            MeterKind::SodaPopper => "resource/ui/huditemeffectmeter_sodapopper.res",
            MeterKind::Cleaver => "resource/ui/huditemeffectmeter_cleaver.res",
            MeterKind::Minigun => "resource/ui/huditemeffectmeter_heavy.res",
            MeterKind::Heads => "resource/ui/huditemeffectmeter_sniper.res",
            MeterKind::Focus => "resource/ui/huditemeffectmeter_sniperfocus.res",
            MeterKind::Sword => "resource/ui/huditemeffectmeter_demoman.res",
            MeterKind::CowMangler => "resource/ui/huditemeffectmeter_particlecannon.res",
            MeterKind::Bison => "resource/ui/huditemeffectmeter_raygun.res",
            MeterKind::Spycicle => "resource/ui/huditemeffectmeter_spyknife.res",
            MeterKind::Sapper => "resource/ui/huditemeffectmeter_sapper.res",
            MeterKind::Revolver => "resource/ui/huditemeffectmeter_spy.res",
            MeterKind::Revenge => "resource/ui/huditemeffectmeter_engineer.res",
            MeterKind::Pomson => "resource/ui/huditemeffectmeter_pomson.res",
            MeterKind::Flamethrower => "resource/ui/huditemeffectmeter_pyro.res",
            MeterKind::RocketPack => "resource/ui/hudrocketpack.res",
            MeterKind::Organs => "resource/ui/huditemeffectmeter_organs.res",
            MeterKind::Killstreak => "resource/ui/huditemeffectmeter_killstreak.res",
            MeterKind::Kart => "resource/ui/huditemeffectmeter_kartcharge.res",
            MeterKind::Canteen => "resource/ui/huditemeffectmeter_powerupbottle.res",
        }
    }

    /// The kind whose file is `path`, compared ASCII case-insensitively.
    pub fn for_file(path: &str) -> Option<MeterKind> {
        MeterKind::ALL
            .into_iter()
            .find(|kind| kind.file().eq_ignore_ascii_case(path))
    }
}
