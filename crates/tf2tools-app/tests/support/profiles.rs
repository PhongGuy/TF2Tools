//! A library with two HUDs, two sounds and three mastercomfig add-ons, for the profile tests.

use std::collections::BTreeMap;
use std::path::PathBuf;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, Entity, TestAppContext};
use tf2tools_app::ids;
use tf2tools_app::pages::profiles::ProfilesPage;
use tf2tools_app::pages::profiles::draft_editor::ProfileDraftEditor;
use tf2tools_app::pages::profiles::probe::Tf2Probe;
use tf2tools_app::services::Services;
use tf2tools_app::shell::route::Page;
use tf2tools_core::{
    AddonName, Choices, CrosshairRef, CurrentSetup, DraftCrosshair, DraftItem, HudEntry,
    ModuleChoice, Preset, Profile, ProfileDraft, ProfileId, Provenance, SlotMode, SoundEntry,
    Tf2Install,
};

pub const PYRO: &str = "mastercomfig-addon-no-pyroland.vpk";
pub const FLAT: &str = "mastercomfig-addon-flat-mouse.vpk";
pub const NULL: &str = "mastercomfig-addon-null-canceling-movement.vpk";

pub fn addon(name: &str) -> AddonName {
    AddonName::new(name).unwrap()
}

/// An add-on's asset name without its prefix and extension, standing in for its display name.
pub fn label(name: &AddonName) -> String {
    let s = name.as_str();
    s.trim_start_matches("mastercomfig-addon-")
        .trim_end_matches(".vpk")
        .to_owned()
}

/// Low with fpscap 60, one change from the preset in the trimmed catalog.
pub fn low_fps60() -> Choices {
    let mut choices = Choices::new(Preset::Low);
    choices.set(
        &super::module_catalog(),
        "fpscap",
        ModuleChoice::Value("60".into()),
    );
    choices
}

pub fn on_scattergun(crosshair: CrosshairRef) -> DraftCrosshair {
    DraftCrosshair::Set {
        weapon_script: "tf_weapon_scattergun".into(),
        crosshair,
    }
}

pub struct Fixture {
    pub env: super::Env,
    pub s: Services,
    pub rayshud: HudEntry,
    pub budhud: HudEntry,
    pub ding: SoundEntry,
    pub bell: SoundEntry,
}

pub fn fixture() -> Fixture {
    let env = super::env();
    let s = super::services(&env, super::no_steam());
    let hud = |name: &str| {
        let path = env.tmp.path().join(format!("{name}.zip"));
        std::fs::write(&path, super::hud_zip_bytes(name)).unwrap();
        s.library.import_hud(&path, Provenance::Local).unwrap()
    };
    let sound = |name: &str| {
        let path = env.tmp.path().join(format!("{name}.wav"));
        std::fs::write(&path, super::wav_bytes()).unwrap();
        s.library
            .import_sound(&path, name, Provenance::Local)
            .unwrap()
    };
    let vpk = super::vpk_bytes(&[("cfg/addon.cfg", b"echo addon\n")]);
    for name in [PYRO, FLAT, NULL] {
        super::seed_library_addon(&s, &env, name, "9.100.1", &vpk, false);
    }
    let (rayshud, budhud) = (hud("rayshud"), hud("budhud"));
    let (ding, bell) = (sound("Ding"), sound("Bell"));
    Fixture {
        env,
        s,
        rayshud,
        budhud,
        ding,
        bell,
    }
}

impl Fixture {
    pub fn tf2(&self) -> Tf2Install {
        Tf2Install::from_path(&self.env.game).unwrap()
    }

    pub fn builtin(&self, ix: usize) -> CrosshairRef {
        CrosshairRef::Builtin(self.s.builtins.all()[ix].name().to_owned())
    }

    pub fn create(&self, draft: ProfileDraft) -> Profile {
        self.s.library.create_profile(&draft).unwrap()
    }

    /// Every slot Set: rayshud, Ding and Bell, builtin 0 on the scattergun, Low with fpscap 60,
    /// and no-pyroland with Only these.
    pub fn full(&self, name: &str) -> Profile {
        self.create(
            ProfileDraft::new(name)
                .with_hud(SlotMode::Set(DraftItem::Pick(self.rayshud.id().clone())))
                .with_sounds(SlotMode::Set((
                    Some(DraftItem::Pick(self.ding.id().clone())),
                    Some(DraftItem::Pick(self.bell.id().clone())),
                )))
                .with_crosshairs(SlotMode::Set(vec![on_scattergun(self.builtin(0))]))
                .with_graphics(SlotMode::Set(low_fps60()))
                .with_addons(SlotMode::Set((vec![DraftItem::Pick(addon(PYRO))], true))),
        )
    }

    /// Every slot None (`none`) or every slot Leave.
    pub fn every(&self, name: &str, none: bool) -> Profile {
        fn mode<T>(none: bool) -> SlotMode<T> {
            if none {
                SlotMode::None
            } else {
                SlotMode::Leave
            }
        }
        self.create(
            ProfileDraft::new(name)
                .with_hud(mode(none))
                .with_sounds(mode(none))
                .with_crosshairs(mode(none))
                .with_graphics(mode(none))
                .with_addons(mode(none)),
        )
    }

    /// Applies "Base" (rayshud, Ding and Bell, builtin 0 on the scattergun), then manages
    /// mastercomfig on Low with fpscap 60 and turns no-pyroland on. Returns what is installed.
    pub fn current_after_base(&self) -> CurrentSetup {
        let tf2 = self.tf2();
        let base = self.create(
            ProfileDraft::new("Base")
                .with_hud(SlotMode::Set(DraftItem::Pick(self.rayshud.id().clone())))
                .with_sounds(SlotMode::Set((
                    Some(DraftItem::Pick(self.ding.id().clone())),
                    Some(DraftItem::Pick(self.bell.id().clone())),
                )))
                .with_crosshairs(SlotMode::Set(vec![on_scattergun(self.builtin(0))]))
                .with_graphics(SlotMode::Leave)
                .with_addons(SlotMode::Leave),
        );
        self.s
            .library
            .apply_profile(base.id(), &tf2, &self.s.builtins, false)
            .unwrap();
        super::seed_managed_mastercomfig(&self.env, "9.100.1", "low");
        super::seed_mastercomfig_choices(&self.env, r#"{"fpscap":"60"}"#);
        self.s.library.install_addon(&addon(PYRO), &tf2).unwrap();
        self.s.library.current_setup(&tf2).unwrap()
    }
}

pub struct Seeded {
    pub rayshud: HudEntry,
    pub budhud: HudEntry,
    pub ding: SoundEntry,
    pub bell: SoundEntry,
}

/// The fixture's two HUDs and two sounds, imported through a shell window's services.
pub fn seed(s: &super::ShellWindow, env: &super::Env) -> Seeded {
    let hud = |name: &str| {
        let path = env.tmp.path().join(format!("{name}.zip"));
        std::fs::write(&path, super::hud_zip_bytes(name)).unwrap();
        s.services
            .library
            .import_hud(&path, Provenance::Local)
            .unwrap()
    };
    let sound = |name: &str| {
        let path = env.tmp.path().join(format!("{name}.wav"));
        std::fs::write(&path, super::wav_bytes()).unwrap();
        s.services
            .library
            .import_sound(&path, name, Provenance::Local)
            .unwrap()
    };
    Seeded {
        rayshud: hud("rayshud"),
        budhud: hud("budhud"),
        ding: sound("Ding"),
        bell: sound("Bell"),
    }
}

/// Every file under the game folder with its bytes, to prove the editor wrote nothing.
pub fn game_files(env: &super::Env) -> BTreeMap<PathBuf, Vec<u8>> {
    fn walk(dir: &std::path::Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out)
            } else {
                out.insert(path.clone(), std::fs::read(&path).unwrap());
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(&env.game, &mut out);
    out
}

pub fn page(cx: &mut TestAppContext, s: &super::ShellWindow) -> Entity<ProfilesPage> {
    cx.executor().allow_parking();
    cx.update(|cx| Tf2Probe::install(cx, || false));
    super::click_id(cx, s.win, ids::nav(Page::Profiles));
    let page = cx.update(|cx| s.shell.read(cx).profiles_page()).unwrap();
    super::wait_real(cx, "Profiles to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    page
}

fn opened(
    cx: &mut TestAppContext,
    s: &super::ShellWindow,
    page: &Entity<ProfilesPage>,
) -> Entity<ProfileDraftEditor> {
    super::wait_real(cx, "the editor", |cx| {
        cx.update(|cx| page.read(cx).editor().is_some())
    });
    super::settle(cx, s.win);
    cx.update(|cx| page.read(cx).editor()).unwrap()
}

pub fn edit(
    cx: &mut TestAppContext,
    s: &super::ShellWindow,
    page: &Entity<ProfilesPage>,
    id: &ProfileId,
) -> Entity<ProfileDraftEditor> {
    super::click_id(cx, s.win, ids::profile_card_edit(id));
    opened(cx, s, page)
}

pub fn new_draft(
    cx: &mut TestAppContext,
    s: &super::ShellWindow,
    page: &Entity<ProfilesPage>,
) -> Entity<ProfileDraftEditor> {
    super::click_id(cx, s.win, ids::PROFILES_NEW);
    opened(cx, s, page)
}

pub fn type_name(
    cx: &mut TestAppContext,
    s: &super::ShellWindow,
    e: &Entity<ProfileDraftEditor>,
    text: &str,
) {
    let input = cx.update(|cx| e.read(cx).name_input().clone());
    cx.update_window(s.win, |_, window, cx| {
        input.update(cx, |st, cx| {
            st.focus(window, cx);
            st.set_value(text, window, cx)
        })
    })
    .unwrap();
    super::settle(cx, s.win);
}

/// Wheels the editor body to its foot and clicks Delete, as a player scrolls down to it.
pub fn click_delete(cx: &mut TestAppContext, s: &super::ShellWindow) {
    let down = gpui_kit::ScrollDelta::Lines(gpui_kit::point(0., -1000.));
    cx.update_window(s.win, |_, w, cx| {
        w.scroll(ids::PROFILE_EDITOR_BODY, down, cx)
    })
    .unwrap();
    super::settle(cx, s.win);
    super::click_id(cx, s.win, ids::PROFILE_EDITOR_DELETE);
}
