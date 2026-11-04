//! Weapon icons from the TF2 wiki: which item pictures each weapon script, and its URL.

mod common;

use common::{fast_config, store_env};
use tf2tools_core::{Database, Endpoints, StoreClient, Weapon, weapons};
use wiremock::matchers::{header_regex, method, path};
use wiremock::{Mock, ResponseTemplate};

// Each name was checked to exist as `Item_icon_<name>.png` on the wiki.
const NAMES: [(&str, &str); 64] = [
    ("tf_weapon_pep_brawler_blaster", "Baby Face's Blaster"),
    ("tf_weapon_lunchbox_drink", "Bonk! Atomic Punch"),
    ("tf_weapon_bat_fish", "Holy Mackerel"),
    ("tf_weapon_jar_milk", "Mad Milk"),
    ("tf_weapon_bat", "Bat"),
    ("tf_weapon_bat_wood", "Sandman"),
    ("tf_weapon_scattergun", "Scattergun"),
    ("tf_weapon_handgun_scout_primary", "Shortstop"),
    ("tf_weapon_soda_popper", "Soda Popper"),
    ("tf_weapon_pistol_scout", "Pistol"),
    ("tf_weapon_handgun_scout_secondary", "Winger"),
    ("tf_weapon_bat_giftwrap", "Wrap Assassin"),
    ("tf_weapon_cleaver", "Flying Guillotine"),
    ("tf_weapon_rocketlauncher_airstrike", "Air Strike"),
    ("tf_weapon_buff_item", "Buff Banner"),
    ("tf_weapon_particle_cannon", "Cow Mangler 5000"),
    ("tf_weapon_rocketlauncher_directhit", "Direct Hit"),
    ("tf_weapon_shovel", "Shovel"),
    ("tf_weapon_raygun", "Righteous Bison"),
    ("tf_weapon_rocketlauncher", "Rocket Launcher"),
    ("tf_weapon_shotgun_soldier", "Shotgun"),
    ("tf_weapon_rocketlauncher_fireball", "Dragon's Fury"),
    ("tf_weapon_flamethrower", "Flame Thrower"),
    ("tf_weapon_flaregun", "Flare Gun"),
    ("tf_weapon_slap", "Hot Hand"),
    ("tf_weapon_flaregun_revenge", "Manmelter"),
    ("tf_weapon_fireaxe", "Fire Axe"),
    ("tf_weapon_shotgun_pyro", "Shotgun"),
    ("tf_weapon_rocketpack", "Thermal Thruster"),
    ("tf_weapon_grenadelauncher", "Grenade Launcher"),
    ("tf_weapon_cannon", "Loose Cannon"),
    ("tf_weapon_bottle", "Bottle"),
    ("tf_weapon_katana", "Half-Zatoichi"),
    ("tf_weapon_pipebomblauncher", "Stickybomb Launcher"),
    ("tf_weapon_sword", "Eyelander"),
    ("tf_weapon_stickbomb", "Ullapool Caber"),
    ("tf_weapon_fists", "Fists"),
    ("tf_weapon_minigun", "Minigun"),
    ("tf_weapon_lunchbox", "Sandvich"),
    ("tf_weapon_shotgun_hwg", "Shotgun"),
    ("tf_weapon_robot_arm", "Gunslinger"),
    ("tf_weapon_wrench", "Wrench"),
    ("tf_weapon_pistol", "Pistol"),
    ("tf_weapon_shotgun_building_rescue", "Rescue Ranger"),
    ("tf_weapon_mechanical_arm", "Short Circuit"),
    ("tf_weapon_shotgun_primary", "Shotgun"),
    ("tf_weapon_sentry_revenge", "Frontier Justice"),
    ("tf_weapon_laser_pointer", "Wrangler"),
    ("tf_weapon_crossbow", "Crusader's Crossbow"),
    ("tf_weapon_medigun", "Medi Gun"),
    ("tf_weapon_bonesaw", "Bonesaw"),
    ("tf_weapon_syringegun_medic", "Syringe Gun"),
    ("tf_weapon_compound_bow", "Huntsman"),
    ("tf_weapon_sniperrifle_classic", "Classic"),
    ("tf_weapon_charged_smg", "Cleaner's Carbine"),
    ("tf_weapon_jar", "Jarate"),
    ("tf_weapon_club", "Kukri"),
    ("tf_weapon_sniperrifle", "Sniper Rifle"),
    ("tf_weapon_smg", "SMG"),
    ("tf_weapon_sniperrifle_decap", "Bazaar Bargain"),
    ("tf_weapon_sapper", "Red-Tape Recorder"),
    ("tf_weapon_knife", "Knife"),
    ("tf_weapon_revolver", "Revolver"),
    ("tf_weapon_builder", "Sapper"),
];

fn weapon(script: &str) -> &'static Weapon {
    weapons()
        .iter()
        .find(|w| w.script_name() == script)
        .unwrap_or_else(|| panic!("no weapon {script}"))
}

#[test]
fn every_weapon_is_pictured_by_its_checked_wiki_item() {
    let got: Vec<(&str, Option<&str>)> = weapons()
        .iter()
        .map(|w| (w.script_name(), w.icon_name()))
        .collect();
    let want: Vec<(&str, Option<&str>)> = NAMES.iter().map(|&(s, n)| (s, Some(n))).collect();
    assert_eq!(got, want);
}

#[test]
fn wiki_names_never_lead_with_the() {
    for w in weapons() {
        let name = w.icon_name().unwrap();
        assert!(!name.starts_with("The "), "{}: {name}", w.script_name());
    }
}

#[test]
fn class_shotguns_and_pistols_share_the_stock_picture() {
    for script in [
        "tf_weapon_shotgun_soldier",
        "tf_weapon_shotgun_pyro",
        "tf_weapon_shotgun_hwg",
        "tf_weapon_shotgun_primary",
    ] {
        assert_eq!(weapon(script).icon_name(), Some("Shotgun"), "{script}");
    }
    for script in ["tf_weapon_pistol_scout", "tf_weapon_pistol"] {
        assert_eq!(weapon(script).icon_name(), Some("Pistol"), "{script}");
    }
}

const WIKI: &str = "https://wiki.teamfortress.com/w/images";

// The exact URLs the wiki's own API returned for these files.
#[test]
fn icon_urls_follow_the_wikis_hashed_thumbnail_paths() {
    let e = Endpoints::default();
    assert_eq!(e.wiki_images(), WIKI);
    for (name, url) in [
        (
            "Baby Face's Blaster",
            "thumb/e/e6/Item_icon_Baby_Face%27s_Blaster.png/100px-Item_icon_Baby_Face%27s_Blaster.png",
        ),
        (
            "Bonk! Atomic Punch",
            "thumb/8/8f/Item_icon_Bonk%21_Atomic_Punch.png/100px-Item_icon_Bonk%21_Atomic_Punch.png",
        ),
        (
            "Scattergun",
            "thumb/1/1b/Item_icon_Scattergun.png/100px-Item_icon_Scattergun.png",
        ),
        (
            "Red-Tape Recorder",
            "thumb/7/7d/Item_icon_Red-Tape_Recorder.png/100px-Item_icon_Red-Tape_Recorder.png",
        ),
        (
            "Shotgun",
            "thumb/5/5f/Item_icon_Shotgun.png/100px-Item_icon_Shotgun.png",
        ),
    ] {
        assert_eq!(e.weapon_icon_url(name), format!("{WIKI}/{url}"), "{name}");
    }
}

// MediaWiki hashes the UTF-8 file name and percent-encodes each byte in uppercase hex, so a
// non-ASCII name must keep both; expected paths computed with Python's md5 and quote.
#[test]
fn non_ascii_icon_names_hash_utf8_and_encode_each_byte() {
    let e = Endpoints::default();
    for (name, url) in [
        (
            "Über Saw",
            "thumb/0/01/Item_icon_%C3%9Cber_Saw.png/100px-Item_icon_%C3%9Cber_Saw.png",
        ),
        (
            "Kunai für",
            "thumb/b/b4/Item_icon_Kunai_f%C3%BCr.png/100px-Item_icon_Kunai_f%C3%BCr.png",
        ),
    ] {
        assert_eq!(e.weapon_icon_url(name), format!("{WIKI}/{url}"), "{name}");
    }
}

#[test]
fn production_allows_wiki_images_and_loopback_keeps_them_local() {
    let prod = Endpoints::default();
    let url = prod.weapon_icon_url("Scattergun");
    assert!(
        prod.image_prefixes()
            .iter()
            .any(|p| url.starts_with(p.as_str())),
        "{url}"
    );

    let local = Endpoints::loopback("http://127.0.0.1:9");
    assert_eq!(local.wiki_images(), "http://127.0.0.1:9/img/wiki");
    assert_eq!(local.image_prefixes(), ["http://127.0.0.1:9/img/"]);
    assert_eq!(
        local.weapon_icon_url("Scattergun"),
        "http://127.0.0.1:9/img/wiki/thumb/1/1b/Item_icon_Scattergun.png/100px-Item_icon_Scattergun.png"
    );
    let moved = Endpoints::default().with_wiki_images("http://h/w");
    assert!(
        moved
            .weapon_icon_url("Bat")
            .starts_with("http://h/w/thumb/")
    );
}

#[tokio::test]
async fn the_client_names_each_weapons_icon_url() {
    let env = store_env().await;
    let scattergun = weapon("tf_weapon_scattergun");
    assert_eq!(
        env.client.weapon_icon_url(scattergun).as_deref(),
        Some(
            format!("{WIKI}/thumb/1/1b/Item_icon_Scattergun.png/100px-Item_icon_Scattergun.png")
                .as_str()
        )
    );
}

// The icon comes through the store's image fetch, so it is allowlisted, cached and sent with
// the generic User-Agent (never an email address).
#[tokio::test]
async fn an_icon_is_fetched_from_the_wiki_root_with_the_generic_user_agent() {
    let env = store_env().await;
    let uri = env.server.uri();
    let png = b"\x89PNG not really".to_vec();
    Mock::given(method("GET"))
        .and(path(
            "/img/wiki/thumb/1/1b/Item_icon_Scattergun.png/100px-Item_icon_Scattergun.png",
        ))
        .and(header_regex("user-agent", r"^TF2Tools/[^@]*$"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(png.clone()))
        .expect(1)
        .mount(&env.server)
        .await;
    let client = StoreClient::new(
        Database::open(&env.dir).unwrap(),
        env.dir.clone(),
        Endpoints::loopback(&uri),
    )
    .with_config(fast_config());
    let url = client
        .weapon_icon_url(weapon("tf_weapon_scattergun"))
        .unwrap();
    assert_eq!(client.fetch_image(&url).await.unwrap(), png);
}
