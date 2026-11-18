//! The library pages, the names they share and the navigation, in Russian.

use std::io::Write as _;
use std::path::{Path, PathBuf};

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, SharedString, TestAppContext};
use tf2tools_app::crosshair_sync::installed_text;
use tf2tools_app::gb_addon_installs::GbAddonOffer;
use tf2tools_app::i18n::names::addon_section_name;
use tf2tools_app::i18n::{self, names};
use tf2tools_app::ids;
use tf2tools_app::mature::MatureAddons;
use tf2tools_app::pages::addon_store::card::CardBadge;
use tf2tools_app::pages::addons::AddonsPage;
use tf2tools_app::pages::addons::model::{movement_off, movement_on_text};
use tf2tools_app::pages::crosshairs::CrosshairsPage;
use tf2tools_app::pages::huds::HudsPage;
use tf2tools_app::pages::huds::copy_name;
use tf2tools_app::pages::maps::model::{SORTS, last_played_label, sort_label};
use tf2tools_app::pages::profiles::ProfilesPage;
use tf2tools_app::pages::profiles::editor_model::{Slot, SlotState, next_profile_name, slot_text};
use tf2tools_app::pages::profiles::model::{crosshair_count, not_applied};
use tf2tools_app::pages::profiles::probe::Tf2Probe;
use tf2tools_app::pages::sounds::SoundsPage;
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::confirm::Confirm;
use tf2tools_app::shell::route::Page;
use tf2tools_app::sync_messages::describe_sync;
use tf2tools_app::widgets::class_icon::class_short_name;
use tf2tools_app::widgets::profile_menu::added_title;
use tf2tools_app::widgets::sync_banner::maps_banner_text;
use tf2tools_core::{
    AddonSection, DraftSlot, GbId, ProfileDraft, Provenance, SoundKind, StoreKind, SyncError,
    Tf2Install, TfClass, WeaponSlot,
};

use crate::LocaleGuard;
use crate::support;

fn found(env: &support::Env) -> Tf2State {
    Tf2State::Found(Tf2Install::from_path(&env.game).unwrap())
}

// The HUDs page, the shell's start page, once its first load is done.
fn open_huds(cx: &mut TestAppContext, s: &support::ShellWindow) -> Entity<HudsPage> {
    let page = cx
        .update(|cx| s.shell.read(cx).huds_page())
        .expect("a HUDs page");
    support::wait_real(cx, "HUDs to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    support::settle(cx, s.win);
    page
}

// The Sounds page, once its load and the saved volume are in.
fn open_sounds(cx: &mut TestAppContext, s: &support::ShellWindow) -> Entity<SoundsPage> {
    support::click_id(cx, s.win, ids::nav(Page::Sounds));
    let page = cx
        .update(|cx| s.shell.read(cx).sounds_page())
        .expect("a Sounds page");
    support::wait_real(cx, "Sounds to load", |cx| {
        cx.update(|cx| {
            let page = page.read(cx);
            page.is_loaded() && page.player().read(cx).is_ready()
        })
    });
    support::settle(cx, s.win);
    page
}

// The Crosshairs page, once its first load is done.
fn open_crosshairs(cx: &mut TestAppContext, s: &support::ShellWindow) -> Entity<CrosshairsPage> {
    support::click_id(cx, s.win, ids::nav(Page::Crosshairs));
    let page = cx
        .update(|cx| s.shell.read(cx).crosshairs_page())
        .expect("a Crosshairs page");
    support::wait_real(cx, "Crosshairs to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    support::settle(cx, s.win);
    page
}

fn search(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<CrosshairsPage>,
    text: &str,
) {
    let input = cx.update(|cx| page.read(cx).search_input().clone());
    cx.update_window(s.win, |_, window, cx| {
        input.update(cx, |st, cx| st.set_value(text, window, cx))
    })
    .unwrap();
    support::settle(cx, s.win);
}

// The Profiles page with the TF2 probe pinned to "not running", once its load is done.
fn open_profiles(cx: &mut TestAppContext, s: &support::ShellWindow) -> Entity<ProfilesPage> {
    cx.update(|cx| Tf2Probe::install(cx, || false));
    support::click_id(cx, s.win, ids::nav(Page::Profiles));
    let page = cx
        .update(|cx| s.shell.read(cx).profiles_page())
        .expect("a Profiles page");
    support::wait_real(cx, "Profiles to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    support::settle(cx, s.win);
    page
}

fn toast_is(cx: &mut TestAppContext, s: &support::ShellWindow, text: &str) {
    support::wait_real(cx, text, |cx| {
        support::label_of(cx, s.win, ids::TOAST).as_deref() == Some(text)
    });
}

// A confirm dialog's four texts, in reading order.
fn texts(c: &Confirm) -> [&str; 4] {
    [
        c.title.as_ref(),
        c.description.as_ref(),
        c.confirm_label.as_ref(),
        c.cancel_label.as_ref(),
    ]
}

// A minimal HUD archive, `<name>/info.vdf` plus a layout file, written into `dir`.
fn hud_zip(dir: &Path, name: &str) -> PathBuf {
    let path = dir.join(format!("{name}.zip"));
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    let options = zip::write::SimpleFileOptions::default();
    zip.start_file(format!("{name}/info.vdf"), options).unwrap();
    write!(zip, "\"{name}\"\n{{\n\t\"ui_version\"\t\"3\"\n}}\n").unwrap();
    zip.start_file(format!("{name}/resource/ui/hudlayout.res"), options)
        .unwrap();
    write!(zip, "\"Resource/HudLayout.res\"\n{{\n}}\n").unwrap();
    zip.finish().unwrap();
    path
}

fn classes(name: fn(TfClass) -> SharedString) -> Vec<String> {
    TfClass::ALL.iter().map(|c| name(*c).to_string()).collect()
}

#[test]
fn class_slot_and_sound_names_read_in_both_languages() {
    let _guard = LocaleGuard::hold();
    assert_eq!(names::class_name(TfClass::Heavy), "Heavy");
    assert_eq!(class_short_name(TfClass::Engineer), "Engie");
    assert_eq!(
        classes(names::class_compact_name),
        classes(names::class_name)
    );
    assert_eq!(names::slot_name(WeaponSlot::Melee), "Melee");
    assert_eq!(names::sound_kind_name(SoundKind::Killsound), "Killsound");
    i18n::apply("ru");
    assert_eq!(
        classes(names::class_name),
        [
            "Разведчик",
            "Солдат",
            "Поджигатель",
            "Подрывник",
            "Пулемётчик",
            "Инженер",
            "Медик",
            "Снайпер",
            "Шпион"
        ]
    );
    assert_eq!(
        classes(class_short_name),
        [
            "Развед.",
            "Солдат",
            "Поджиг.",
            "Подрыв.",
            "Пулем.",
            "Инж.",
            "Медик",
            "Снайпер",
            "Шпион"
        ]
    );
    assert_eq!(
        classes(names::class_compact_name),
        [
            "Развед.",
            "Солдат",
            "Поджиг.",
            "Подрыв.",
            "Пулем.",
            "Инженер",
            "Медик",
            "Снайпер",
            "Шпион"
        ]
    );
    let slots = [
        WeaponSlot::Primary,
        WeaponSlot::Secondary,
        WeaponSlot::Melee,
    ]
    .map(|s| names::slot_name(s).to_string());
    assert_eq!(slots, ["Основное", "Дополнительное", "Ближний бой"]);
    let kinds =
        [SoundKind::Hitsound, SoundKind::Killsound].map(|k| names::sound_kind_name(k).to_string());
    assert_eq!(kinds, ["Хитсаунд", "Килсаунд"]);
}

#[gpui_kit::test]
fn the_sidebar_header_and_tf2_notice_read_in_russian(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("ru");
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    let label = |cx: &mut TestAppContext, id: gpui_kit::ElementId| support::label_of(cx, s.win, id);
    assert_eq!(label(cx, ids::nav(Page::Huds)).as_deref(), Some("HUD"));
    assert_eq!(
        label(cx, ids::nav(Page::Crosshairs)).as_deref(),
        Some("Прицелы")
    );
    assert_eq!(
        label(cx, ids::nav(Page::AddonStore)).as_deref(),
        Some("Магазин дополнений")
    );
    assert_eq!(label(cx, ids::HEADER_TITLE.into()).as_deref(), Some("HUD"));
    support::wait_real(cx, "the no-TF2 notice", |cx| {
        support::present(cx, s.win, ids::HUDS_NO_TF2)
    });
    assert_eq!(
        label(cx, ids::HUDS_NO_TF2.into()).as_deref(),
        Some("TF2 не найдена")
    );
    support::click_id(cx, s.win, ids::nav(Page::Logs));
    assert_eq!(
        label(cx, ids::HEADER_TITLE.into()).as_deref(),
        Some("Журнал действий")
    );
}

#[gpui_kit::test]
fn the_huds_page_reads_in_russian(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("ru");
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_huds(cx, &s);
    assert_eq!(
        support::label_of(cx, s.win, ids::HUDS_INSTALLED_EMPTY).as_deref(),
        Some("HUD не установлен")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::HUDS_EMPTY).as_deref(),
        Some("Ваша библиотека пуста")
    );
    let archive = hud_zip(env.tmp.path(), "budhud");
    s.services
        .library
        .import_hud(&archive, Provenance::Local)
        .unwrap();
    support::click_id(cx, s.win, ids::nav(Page::Settings));
    support::click_id(cx, s.win, ids::nav(Page::Huds));
    let _page = open_huds(cx, &s);
    assert_eq!(
        support::label_of(cx, s.win, ids::hud_badge("budhud")).as_deref(),
        Some("Свой")
    );
    support::click_id(cx, s.win, ids::hud_delete("budhud"));
    assert_eq!(
        support::label_of(cx, s.win, ids::CONFIRM_TITLE).as_deref(),
        Some("Удалить HUD")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::CONFIRM_DESCRIPTION).as_deref(),
        Some("Удалить budhud из библиотеки? Это действие нельзя отменить.")
    );
}

// `text` shaped in the theme font at `size` rems and `weight`, as the window lays it out.
pub fn text_width(
    cx: &mut TestAppContext,
    win: AnyWindowHandle,
    text: &str,
    size: f32,
    weight: gpui_kit::FontWeight,
) -> gpui_kit::Pixels {
    use gpui_kit::component::ActiveTheme as _;
    cx.update_window(win, |_, w, cx| {
        let font = gpui_kit::Font {
            weight,
            ..gpui_kit::font(cx.theme().font_family.clone())
        };
        let run = gpui_kit::TextRun {
            len: text.len(),
            font,
            ..Default::default()
        };
        let size = gpui_kit::rems(size).to_pixels(w.rem_size());
        w.text_system()
            .shape_line(text.to_owned().into(), size, &[run], None)
            .width
    })
    .unwrap()
}

fn resize_800(cx: &mut TestAppContext, s: &support::ShellWindow) {
    cx.simulate_window_resize(
        s.win,
        gpui_kit::size(gpui_kit::px(800.), gpui_kit::px(600.)),
    );
    support::settle(cx, s.win);
}

// `inner` lies within `outer`, left and right.
fn assert_inside(
    what: &str,
    inner: gpui_kit::Bounds<gpui_kit::Pixels>,
    outer: gpui_kit::Bounds<gpui_kit::Pixels>,
) {
    assert!(
        inner.left() >= outer.left() && inner.right() <= outer.right(),
        "{what} {inner:?} runs past {outer:?}"
    );
}

// A HUDs page at 800x600 with budhud, a local HUD, in the library.
fn huds_with_budhud(cx: &mut TestAppContext, env: &support::Env) -> support::ShellWindow {
    let s = support::shell_window(cx, env, found(env));
    let archive = hud_zip(env.tmp.path(), "budhud");
    s.services
        .library
        .import_hud(&archive, Provenance::Local)
        .unwrap();
    resize_800(cx, &s);
    support::click_id(cx, s.win, ids::nav(Page::Settings));
    support::click_id(cx, s.win, ids::nav(Page::Huds));
    open_huds(cx, &s);
    s
}

// An icon-only button is a 32 px square; a labelled one is wider.
fn icon_only(cx: &mut TestAppContext, win: AnyWindowHandle, id: gpui_kit::ElementId) -> bool {
    support::bounds_of(cx, win, id).unwrap().size.width <= gpui_kit::px(32.)
}

// English keeps the card's Install label; the longer Russian one leaves the icon, named
// "Установить" for its tooltip and screen readers.
#[gpui_kit::test]
fn the_hud_card_install_keeps_its_label_in_english(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::hold();
    let env = support::env();
    let s = huds_with_budhud(cx, &env);
    let install = ids::hud_install("budhud");
    assert_eq!(
        support::label_of(cx, s.win, install.clone()).as_deref(),
        Some("Install")
    );
    assert!(!icon_only(cx, s.win, install));
}

#[gpui_kit::test]
fn the_russian_hud_card_install_is_a_named_icon(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("ru");
    let env = support::env();
    let s = huds_with_budhud(cx, &env);
    let install = ids::hud_install("budhud");
    assert_eq!(
        support::label_of(cx, s.win, install.clone()).as_deref(),
        Some("Установить")
    );
    assert!(icon_only(cx, s.win, install));
}

// At 800x600 a library card in `lang` keeps its action row inside the card and room for the
// HUD's name, and the search box shows its whole placeholder.
fn hud_card_fits_at_800(cx: &mut TestAppContext, lang: &str) {
    let _guard = LocaleGuard::switch(lang);
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let archive = hud_zip(env.tmp.path(), "budhud");
    s.services
        .library
        .import_hud(&archive, Provenance::Local)
        .unwrap();
    resize_800(cx, &s);
    support::click_id(cx, s.win, ids::nav(Page::Settings));
    support::click_id(cx, s.win, ids::nav(Page::Huds));
    let page = open_huds(cx, &s);
    let card = support::bounds_of(cx, s.win, ids::hud_card("budhud")).unwrap();
    for id in [
        ids::hud_copy("budhud"),
        ids::hud_delete("budhud"),
        ids::hud_install("budhud"),
    ] {
        let b = support::bounds_of(cx, s.win, id.clone()).unwrap();
        assert_inside(&format!("{lang} {id:?}"), b, card);
    }
    // The name starts after the card's 24 px padding and keeps 8 px before the badge group.
    let menu = support::bounds_of(cx, s.win, ids::hud_add_to_profile("budhud")).unwrap();
    let room = menu.left() - card.left() - gpui_kit::px(32.);
    let name = text_width(cx, s.win, "budhud", 1.0, gpui_kit::FontWeight::SEMIBOLD);
    assert!(name <= room, "{lang}: budhud needs {name:?}, has {room:?}");
    // The box's search icon and paddings take 44 px of its width.
    let placeholder = cx.update(|cx| {
        page.read(cx)
            .search_input()
            .read(cx)
            .presentation()
            .placeholder()
            .to_string()
    });
    let text = text_width(cx, s.win, &placeholder, 0.875, gpui_kit::FontWeight::NORMAL);
    let search = support::bounds_of(cx, s.win, ids::HUDS_SEARCH).unwrap();
    assert!(
        text + gpui_kit::px(44.) <= search.size.width,
        "{lang}: {placeholder} needs {text:?} in {search:?}"
    );
}

#[gpui_kit::test]
fn the_russian_hud_card_fits_at_800(cx: &mut TestAppContext) {
    hud_card_fits_at_800(cx, "ru");
}

#[gpui_kit::test]
fn the_portuguese_hud_card_fits_at_800(cx: &mut TestAppContext) {
    hud_card_fits_at_800(cx, "pt-BR");
}

// At 800x600 a profile card in `lang` keeps Activate, Edit and Delete inside the card, and
// Activate, icon-only there, keeps its name.
fn profile_card_fits_at_800(cx: &mut TestAppContext, lang: &str) {
    let _guard = LocaleGuard::switch(lang);
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let comp = s
        .services
        .library
        .create_profile(&ProfileDraft::new("Competitive"))
        .unwrap();
    resize_800(cx, &s);
    let _page = open_profiles(cx, &s);
    let id = comp.id();
    let card = support::bounds_of(cx, s.win, ids::profile_card(id)).unwrap();
    for button in [
        ids::profile_activate(id),
        ids::profile_edit(id),
        ids::profile_delete(id),
    ] {
        let b = support::bounds_of(cx, s.win, button.clone()).unwrap();
        assert_inside(&format!("{lang} {button:?}"), b, card);
    }
    if lang == "ru" {
        assert_eq!(
            support::label_of(cx, s.win, ids::profile_activate(id)).as_deref(),
            Some("Включить"),
            "Activate keeps its name"
        );
    }
}

#[gpui_kit::test]
fn the_russian_profile_card_fits_at_800(cx: &mut TestAppContext) {
    profile_card_fits_at_800(cx, "ru");
}

#[gpui_kit::test]
fn the_portuguese_profile_card_fits_at_800(cx: &mut TestAppContext) {
    profile_card_fits_at_800(cx, "pt-BR");
}

// The Missing Items dialog calls a local import "Свой", as the HUD card's badge does.
#[gpui_kit::test]
fn a_missing_local_item_reads_as_the_badge_does(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("ru");
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let archive = hud_zip(env.tmp.path(), "budhud");
    let hud = s
        .services
        .library
        .import_hud(&archive, Provenance::Local)
        .unwrap();
    let draft = ProfileDraft::new("Comp").with_hud(DraftSlot::Item(hud.id().clone()));
    let comp = s.services.library.create_profile(&draft).unwrap();
    s.services.library.delete_hud(hud.id()).unwrap();
    let _page = open_profiles(cx, &s);
    support::click_id(cx, s.win, ids::profile_activate(comp.id()));
    support::wait_real(cx, "the Missing Items dialog", |cx| {
        support::present(cx, s.win, ids::PROFILES_MISSING_DIALOG)
    });
    assert_eq!(
        support::label_of(cx, s.win, ids::missing_local(0)).as_deref(),
        Some("Свой файл — скачать снова нельзя")
    );
}

// At 800x600 both sound slots in `lang` keep their status badge inside the card, apart from
// the slot's title.
fn sound_slots_fit_at_800(cx: &mut TestAppContext, lang: &str) {
    let _guard = LocaleGuard::switch(lang);
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let dir = env.game.join("tf/custom/tf2tools-sounds/sound/ui");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("hitsound.wav"), support::wav_bytes()).unwrap();
    std::fs::write(dir.join("killsound.wav"), support::wav_bytes()).unwrap();
    resize_800(cx, &s);
    let _page = open_sounds(cx, &s);
    for kind in [SoundKind::Hitsound, SoundKind::Killsound] {
        let card = support::bounds_of(cx, s.win, ids::sound_slot(kind)).unwrap();
        let badge = support::bounds_of(cx, s.win, ids::sound_slot_status(kind)).unwrap();
        // The badge ends inside the card's 24 px padding.
        let inner = gpui_kit::Bounds::from_corners(
            gpui_kit::point(card.left() + gpui_kit::px(24.), card.top()),
            gpui_kit::point(card.right() - gpui_kit::px(24.), card.bottom()),
        );
        assert_inside(&format!("{lang} {kind:?} badge"), badge, inner);
        // Icon (16) and its 8 px gap, then the title, then at least 4 px before the badge.
        let title = names::sound_kind_name(kind).to_string();
        let title_w = text_width(cx, s.win, &title, 1.0, gpui_kit::FontWeight::SEMIBOLD);
        assert!(
            inner.left() + gpui_kit::px(24.) + title_w + gpui_kit::px(4.) <= badge.left(),
            "{lang}: {title} {title_w:?} touches the badge {badge:?}"
        );
    }
}

#[gpui_kit::test]
fn the_russian_sound_slots_fit_at_800(cx: &mut TestAppContext) {
    sound_slots_fit_at_800(cx, "ru");
}

#[gpui_kit::test]
fn the_portuguese_sound_slots_fit_at_800(cx: &mut TestAppContext) {
    sound_slots_fit_at_800(cx, "pt-BR");
}

// At 800x600 an installed slot in `lang` shows its sound's name beside Uninstall: a short
// library name whole, a long one shortened but still readable, never cut to a letter.
fn installed_sound_names_show_at_800(cx: &mut TestAppContext, lang: &str) {
    let _guard = LocaleGuard::switch(lang);
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let path = env.tmp.path().join("ding.wav");
    std::fs::write(&path, support::wav_bytes()).unwrap();
    let ding = s
        .services
        .library
        .import_sound(&path, "Ding", Provenance::Local)
        .unwrap();
    let tf2 = Tf2Install::from_path(&env.game).unwrap();
    s.services
        .library
        .install_sound(ding.id(), SoundKind::Hitsound, &tf2)
        .unwrap();
    let dir = env.game.join("tf/custom/tf2tools-sounds/sound/ui");
    std::fs::write(dir.join("killsound.wav"), support::wav_bytes()).unwrap();
    resize_800(cx, &s);
    let _page = open_sounds(cx, &s);
    for kind in [SoundKind::Hitsound, SoundKind::Killsound] {
        let card = support::bounds_of(cx, s.win, ids::sound_slot(kind)).unwrap();
        let name = support::bounds_of(cx, s.win, ids::sound_slot_name(kind)).unwrap();
        let text = support::label_of(cx, s.win, ids::sound_slot_name(kind)).unwrap();
        let uninstall = support::bounds_of(cx, s.win, ids::sound_slot_uninstall(kind)).unwrap();
        assert_inside(&format!("{lang} {kind:?} uninstall"), uninstall, card);
        assert!(
            name.right() <= uninstall.left(),
            "{lang} {kind:?}: {text:?} {name:?} runs under Uninstall {uninstall:?}"
        );
        let whole = text_width(cx, s.win, &text, 0.875, gpui_kit::FontWeight::NORMAL);
        let floor = whole.min(gpui_kit::px(64.));
        assert!(
            name.size.width + gpui_kit::px(0.5) >= floor,
            "{lang} {kind:?}: {text:?} shows {name:?} of {whole:?}"
        );
    }
    let name = support::bounds_of(cx, s.win, ids::sound_slot_name(SoundKind::Hitsound)).unwrap();
    let whole = text_width(cx, s.win, "Ding", 0.875, gpui_kit::FontWeight::NORMAL);
    assert!(
        name.size.width + gpui_kit::px(0.5) >= whole,
        "{lang}: Ding is cut to {name:?}"
    );
}

#[gpui_kit::test]
fn the_english_installed_sound_names_show_at_800(cx: &mut TestAppContext) {
    installed_sound_names_show_at_800(cx, "en");
}

#[gpui_kit::test]
fn the_russian_installed_sound_names_show_at_800(cx: &mut TestAppContext) {
    installed_sound_names_show_at_800(cx, "ru");
}

#[gpui_kit::test]
fn the_portuguese_installed_sound_names_show_at_800(cx: &mut TestAppContext) {
    installed_sound_names_show_at_800(cx, "pt-BR");
}

// Each slot's status is an icon whose name reads in `lang`, small enough for any word.
fn sound_status_icon_reads(cx: &mut TestAppContext, lang: &str, installed: &str) {
    let _guard = LocaleGuard::switch(lang);
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let dir = env.game.join("tf/custom/tf2tools-sounds/sound/ui");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("killsound.wav"), support::wav_bytes()).unwrap();
    resize_800(cx, &s);
    let _page = open_sounds(cx, &s);
    assert_eq!(
        support::label_of(cx, s.win, ids::sound_slot_status(SoundKind::Killsound)).as_deref(),
        Some(installed),
        "{lang}: the installed status names itself"
    );
    for kind in [SoundKind::Hitsound, SoundKind::Killsound] {
        let badge = support::bounds_of(cx, s.win, ids::sound_slot_status(kind)).unwrap();
        assert!(
            badge.size.width <= gpui_kit::px(28.),
            "{lang} {kind:?}: {badge:?}"
        );
    }
}

#[gpui_kit::test]
fn the_russian_sound_status_is_an_icon(cx: &mut TestAppContext) {
    sound_status_icon_reads(cx, "ru", "В игре");
}

#[gpui_kit::test]
fn the_portuguese_sound_status_is_an_icon(cx: &mut TestAppContext) {
    sound_status_icon_reads(cx, "pt-BR", "Instalado");
}

// At 800x600 a library sound card keeps play, delete, Hit and Kill inside the card in
// `lang`, and Hit and Kill keep their names.
fn assert_sound_card_fits_at_800(cx: &mut TestAppContext, lang: &str, hit: &str, kill: &str) {
    let _guard = LocaleGuard::switch(lang);
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let path = env.tmp.path().join("ding.wav");
    std::fs::write(&path, support::wav_bytes()).unwrap();
    let ding = s
        .services
        .library
        .import_sound(&path, "Ding", Provenance::Local)
        .unwrap();
    resize_800(cx, &s);
    let _page = open_sounds(cx, &s);
    let folder = ding.folder_name();
    let card = support::bounds_of(cx, s.win, ids::sound_card(folder)).unwrap();
    for button in [
        ids::sound_play(folder),
        ids::sound_delete(folder),
        ids::sound_hit(folder),
        ids::sound_kill(folder),
    ] {
        let b = support::bounds_of(cx, s.win, button.clone()).unwrap();
        assert_inside(&format!("{button:?}"), b, card);
    }
    assert_eq!(
        support::label_of(cx, s.win, ids::sound_hit(folder)).as_deref(),
        Some(hit),
        "Hit keeps its name"
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::sound_kill(folder)).as_deref(),
        Some(kill),
        "Kill keeps its name"
    );
}

#[gpui_kit::test]
fn the_sound_card_fits_at_800(cx: &mut TestAppContext) {
    assert_sound_card_fits_at_800(cx, "en", "Hit", "Kill");
}

#[gpui_kit::test]
fn the_russian_sound_card_fits_at_800(cx: &mut TestAppContext) {
    assert_sound_card_fits_at_800(cx, "ru", "Хит", "Кил");
}

#[gpui_kit::test]
fn the_portuguese_sound_card_fits_at_800(cx: &mut TestAppContext) {
    let label = |key: &str| tf2tools_app::_rust_i18n_try_translate("pt-BR", key).unwrap();
    let (hit, kill) = (label("sounds.button.hit"), label("sounds.button.kill"));
    assert_sound_card_fits_at_800(cx, "pt-BR", &hit, &kill);
}

// Where the card has room, at 1280x800, Hit and Kill keep their visible labels.
#[gpui_kit::test]
fn a_wide_sound_card_keeps_its_hit_and_kill_labels(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::hold();
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let path = env.tmp.path().join("ding.wav");
    std::fs::write(&path, support::wav_bytes()).unwrap();
    let ding = s
        .services
        .library
        .import_sound(&path, "Ding", Provenance::Local)
        .unwrap();
    cx.simulate_window_resize(
        s.win,
        gpui_kit::size(gpui_kit::px(1280.), gpui_kit::px(800.)),
    );
    support::settle(cx, s.win);
    let _page = open_sounds(cx, &s);
    let folder = ding.folder_name();
    let card = support::bounds_of(cx, s.win, ids::sound_card(folder)).unwrap();
    for button in [ids::sound_hit(folder), ids::sound_kill(folder)] {
        assert!(
            !icon_only(cx, s.win, button.clone()),
            "{button:?} lost its label"
        );
        let b = support::bounds_of(cx, s.win, button.clone()).unwrap();
        assert_inside(&format!("{button:?}"), b, card);
    }
}

// The translated "no maps yet" line wraps inside the Maps column at 800x600.
fn maps_empty_line_fits_at_800(cx: &mut TestAppContext, lang: &str) {
    let _guard = LocaleGuard::switch(lang);
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Missing { saved: None });
    resize_800(cx, &s);
    support::click_id(cx, s.win, ids::nav(Page::Maps));
    let page = cx.update(|cx| s.shell.read(cx).maps_page()).unwrap();
    support::wait_real(cx, "the cached list", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    let tabs = support::bounds_of(cx, s.win, ids::MAPS_SOURCE_TABS).unwrap();
    let shell = support::bounds_of(cx, s.win, ids::page(Page::Maps)).unwrap();
    let column = gpui_kit::Bounds::from_corners(
        gpui_kit::point(tabs.left(), tabs.top()),
        gpui_kit::point(shell.right() - (tabs.left() - shell.left()), shell.bottom()),
    );
    let empty = support::bounds_of(cx, s.win, ids::MAPS_EMPTY).unwrap();
    assert_inside(&format!("{lang} maps-empty"), empty, column);
}

#[gpui_kit::test]
fn the_russian_maps_empty_line_fits_at_800(cx: &mut TestAppContext) {
    maps_empty_line_fits_at_800(cx, "ru");
}

#[gpui_kit::test]
fn the_portuguese_maps_empty_line_fits_at_800(cx: &mut TestAppContext) {
    maps_empty_line_fits_at_800(cx, "pt-BR");
}

#[test]
fn hud_counts_read_in_russian() {
    let _guard = LocaleGuard::switch("ru");
    let fonts = |n: u64| {
        i18n::counted_plural(
            "font_check.activity.installed",
            n,
            &n.to_string(),
            &[("folder", "budhud")],
        )
    };
    assert_eq!(fonts(1), "Установлен 1 шрифт для HUD: budhud");
    assert_eq!(fonts(2), "Установлено 2 шрифта для HUD: budhud");
    assert_eq!(fonts(5), "Установлено 5 шрифтов для HUD: budhud");
    assert_eq!(fonts(21), "Установлен 21 шрифт для HUD: budhud");
    let more = |n: u64| {
        i18n::counted_plural(
            "external_huds.names_more",
            n,
            &n.to_string(),
            &[("names", "a, b")],
        )
    };
    for n in [1, 2, 5, 21] {
        assert_eq!(more(n), format!("a, b и ещё {n}"));
    }
}

#[gpui_kit::test]
fn the_sounds_page_reads_in_russian(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("ru");
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_sounds(cx, &s);
    assert_eq!(
        support::label_of(cx, s.win, ids::HEADER_TITLE).as_deref(),
        Some("Звуки")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::SOUNDS_EMPTY).as_deref(),
        Some("В библиотеке пока нет звуков")
    );
    let hit = SoundKind::Hitsound;
    assert_eq!(
        support::label_of(cx, s.win, ids::sound_slot_status(hit)).as_deref(),
        Some("Нет")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::sound_slot_name(hit)).as_deref(),
        Some("Хитсаунд не установлен")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::sound_slot_name(SoundKind::Killsound)).as_deref(),
        Some("Килсаунд не установлен")
    );
    assert_eq!(
        texts(&Confirm::delete_sound()),
        [
            "Удалить звук",
            "Удалить этот звук из библиотеки? Это действие нельзя отменить.",
            "Удалить",
            "Отмена"
        ]
    );
    assert_eq!(
        added_title("Ding", "Comp", Some(hit), Some("Old")),
        "Добавлено в профиль Comp как хитсаунд: Ding (вместо Old)"
    );
}

#[gpui_kit::test]
fn the_crosshairs_page_reads_in_russian(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("ru");
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    support::seed_crosshair(&s.services, &env, "Zebra Dot");
    let page = open_crosshairs(cx, &s);
    assert_eq!(
        support::label_of(cx, s.win, ids::CROSSHAIRS_WAITING).as_deref(),
        Some("TF2 использует свои прицелы")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::CROSSHAIRS_PREVIEW).as_deref(),
        Some("Нет прицела")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::CROSSHAIRS_BACKGROUND).as_deref(),
        Some("Тёмный")
    );
    search(cx, &s, &page, "zebra");
    assert_eq!(
        support::label_of(cx, s.win, ids::CROSSHAIRS_GROUP_LIBRARY).as_deref(),
        Some("Мои прицелы (1)")
    );
    search(cx, &s, &page, "zzzz");
    assert_eq!(
        support::label_of(cx, s.win, ids::CROSSHAIRS_NO_MATCH).as_deref(),
        Some("Нет прицелов по вашему запросу")
    );
    assert_eq!(
        texts(&Confirm::delete_crosshair("Zebra Dot", 2, true))[..2],
        [
            "Удалить прицел",
            "Удалить Zebra Dot из библиотеки? Его используют 2 оружия. Они вернутся к прицелу по умолчанию. \
             Он также будет удалён из TF2. Это действие нельзя отменить."
        ]
    );
    assert_eq!(installed_text(1), "Установлено 1 назначение прицела");
    assert_eq!(installed_text(2), "Установлено 2 назначения прицелов");
    assert_eq!(installed_text(5), "Установлено 5 назначений прицелов");
    assert_eq!(installed_text(21), "Установлено 21 назначение прицела");
}

#[gpui_kit::test]
fn the_profiles_page_reads_in_russian(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("ru");
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_profiles(cx, &s);
    assert_eq!(
        support::label_of(cx, s.win, ids::HEADER_TITLE).as_deref(),
        Some("Профили")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::PROFILES_EMPTY).as_deref(),
        Some("Профилей пока нет")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::PROFILES_NO_ACTIVE).as_deref(),
        Some("Нет активного профиля")
    );
    let comp = s
        .services
        .library
        .create_profile(&ProfileDraft::new("Competitive"))
        .unwrap();
    support::click_id(cx, s.win, ids::nav(Page::Settings));
    let _page = open_profiles(cx, &s);
    support::click_id(cx, s.win, ids::profile_delete(comp.id()));
    assert_eq!(
        support::label_of(cx, s.win, ids::CONFIRM_TITLE).as_deref(),
        Some("Удалить профиль")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::CONFIRM_DESCRIPTION).as_deref(),
        Some("Удалить «Competitive»? Это действие нельзя отменить.")
    );
    support::click_id(cx, s.win, ids::CONFIRM_OK);
    toast_is(cx, &s, "Профиль удалён");
}

// At the 800x600 minimum the editor's Crosshairs picker keeps every bulk button in `lang`
// inside it, for the class with the longest class name in `lang` too.
fn bulk_buttons_fit_the_narrow_picker(cx: &mut TestAppContext, lang: &str) {
    let _guard = LocaleGuard::switch(lang);
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let comp = s
        .services
        .library
        .create_profile(&ProfileDraft::new("Competitive"))
        .unwrap();
    let page = open_profiles(cx, &s);
    let id = comp.id().clone();
    cx.update_window(s.win, |_, window, cx| {
        page.update(cx, |p, cx| p.open_editor(id, window, cx))
    })
    .unwrap();
    let editor = cx.update(|cx| page.read(cx).editor()).expect("an editor");
    support::wait_real(cx, "the editor's queue", |cx| {
        cx.update(|cx| editor.read(cx).is_idle())
    });
    support::click_id(cx, s.win, ids::profile_slot("crosshairs"));
    cx.simulate_window_resize(
        s.win,
        gpui_kit::size(gpui_kit::px(800.), gpui_kit::px(600.)),
    );
    support::settle(cx, s.win);
    let longest = TfClass::ALL
        .into_iter()
        .max_by_key(|c| names::class_name(*c).chars().count())
        .unwrap();
    for class in [TfClass::Scout, longest] {
        support::click_id(cx, s.win, ids::class_tab(&class.name().to_lowercase()));
        support::settle(cx, s.win);
        let picker = support::bounds_of(cx, s.win, ids::profile_picker("crosshairs")).unwrap();
        for bulk in [
            ids::CROSSHAIRS_BULK_ALL,
            ids::CROSSHAIRS_BULK_PRIMARY,
            ids::CROSSHAIRS_BULK_SECONDARY,
            ids::CROSSHAIRS_BULK_MELEE,
        ] {
            let b = support::bounds_of(cx, s.win, bulk).unwrap();
            assert!(
                b.left() >= picker.left() && b.right() <= picker.right(),
                "{lang} {class:?}: {bulk} {b:?} runs past the picker {picker:?}"
            );
        }
    }
}

#[gpui_kit::test]
fn the_russian_bulk_buttons_fit_the_narrow_picker(cx: &mut TestAppContext) {
    bulk_buttons_fit_the_narrow_picker(cx, "ru");
}

#[gpui_kit::test]
fn the_portuguese_bulk_buttons_fit_the_narrow_picker(cx: &mut TestAppContext) {
    bulk_buttons_fit_the_narrow_picker(cx, "pt-BR");
}

#[test]
fn profile_texts_read_in_russian() {
    let _guard = LocaleGuard::switch("ru");
    assert_eq!(
        [1, 2, 5, 21].map(crosshair_count),
        ["1 прицел", "2 прицела", "5 прицелов", "21 прицел"]
    );
    assert_eq!(not_applied(), "Изменения ещё не применены");
    assert_eq!(
        texts(&Confirm::tf2_running())[..3],
        [
            "TF2 запущена",
            "TF2 запущена; изменения вступят в силу после перезапуска.",
            "Продолжить"
        ]
    );
    let env = support::env();
    let services = support::services(&env, support::no_steam());
    assert_eq!(next_profile_name(&[]), "Профиль 1");
    for name in ["профиль 1", "Profile 2"] {
        services
            .library
            .create_profile(&ProfileDraft::new(name))
            .unwrap();
    }
    assert_eq!(
        next_profile_name(&services.library.profiles().unwrap()),
        "Профиль 2"
    );
    let labels: Vec<String> = Slot::ALL.iter().map(|s| s.label().to_string()).collect();
    assert_eq!(labels, ["HUD", "Хитсаунд", "Килсаунд", "Прицелы"]);
    assert_eq!(slot_text(&SlotState::Empty, Slot::Hud), "Нет");
    assert_eq!(
        slot_text(&SlotState::Empty, Slot::Crosshairs),
        "Стандартные прицелы"
    );
    assert_eq!(
        slot_text(&SlotState::Missing("budhud".into()), Slot::Hud),
        "Отсутствует: budhud"
    );
    assert_eq!(
        texts(&Confirm::clear_profile_crosshairs())[..3],
        [
            "Очистить прицелы",
            "Убрать все прицелы из этого профиля?",
            "Очистить все"
        ]
    );
}

fn open_addons(cx: &mut TestAppContext, s: &support::ShellWindow) -> Entity<AddonsPage> {
    support::click_id(cx, s.win, ids::nav(Page::Addons));
    let page = cx
        .update(|cx| s.shell.read(cx).addons_page())
        .expect("an Addons page");
    support::wait_real(cx, "Addons to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    support::settle(cx, s.win);
    page
}

#[gpui_kit::test]
fn the_addons_page_reads_in_russian(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("ru");
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let _page = open_addons(cx, &s);
    assert_eq!(
        support::label_of(cx, s.win, ids::HEADER_TITLE).as_deref(),
        Some("Дополнения")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::ADDONS_COUNT).as_deref(),
        Some("В библиотеке нет дополнений")
    );
    let vpk = support::vpk_bytes(&[("cfg/addons/x.cfg", b"x".as_slice())]);
    support::seed_library_addon(
        &s.services,
        &env,
        "mastercomfig-addon-no-pyroland.vpk",
        "9.0.0",
        &vpk,
        false,
    );
    support::click_id(cx, s.win, ids::nav(Page::Settings));
    let _page = open_addons(cx, &s);
    assert_eq!(
        support::label_of(cx, s.win, ids::ADDONS_COUNT).as_deref(),
        Some("В библиотеке: 1, установлено: 0")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::ADDONS_INSTALLED).as_deref(),
        Some("Установленные дополнения")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::ADDONS_LIBRARY).as_deref(),
        Some("Ваша библиотека")
    );
}

#[test]
fn addon_texts_read_in_russian() {
    let _guard = LocaleGuard::switch("ru");
    let keys = |k: &[&str]| k.iter().map(|k| (*k).to_owned()).collect::<Vec<_>>();
    assert_eq!(
        movement_on_text(&keys(&["w", "s"]), &["left", "right"], true),
        "Теперь его используют клавиши: W, S. Ни одна клавиша не двигает вас влево или вправо; \
         назначьте её в TF2. Оно заработает при следующем запуске TF2."
    );
    assert_eq!(
        movement_on_text(&keys(&["UPARROW"]), &[], false),
        "Теперь его используют клавиши: стрелка вверх."
    );
    assert_eq!(
        movement_off(),
        "Клавиши движения снова работают как обычно."
    );
    let sections: Vec<String> = AddonSection::ALL
        .iter()
        .map(|s| addon_section_name(*s).to_string())
        .collect();
    assert_eq!(
        sections,
        ["Скины", "Эффекты", "Интерфейсы", "Текстуры", "Файлы игры"]
    );
    assert_eq!(addon_section_name(AddonSection::MAPS), "Карты");
    assert_eq!(texts(&Confirm::delete_addon())[0], "Удалить дополнение");
}

#[gpui_kit::test]
fn the_maps_page_reads_in_russian(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("ru");
    let (env, server) = (support::env(), support::StoreServer::start());
    let s = support::maps::maps_shell(cx, &env, &server);
    let page = support::maps::open_maps(cx, &s);
    assert_eq!(
        support::label_of(cx, s.win, ids::HEADER_TITLE).as_deref(),
        Some("Карты")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::MAPS_COUNT).as_deref(),
        Some("5 карт")
    );
    let _tab = support::maps::open_gamebanana(cx, &s, &page);
    assert_eq!(
        support::label_of(cx, s.win, ids::MAPS_GB_EMPTY).as_deref(),
        Some("Карты не найдены")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::STORE_EMPTY_HINT).as_deref(),
        Some("Карты появятся здесь после синхронизации с GameBanana.")
    );
}

// The text a wrapped search box or Select shows: its placeholder, else its shown value.
pub fn shown_text(
    cx: &mut TestAppContext,
    win: AnyWindowHandle,
    id: &'static str,
) -> Option<String> {
    support::settle(cx, win);
    cx.update_window(win, |_, w, _| {
        w.within(id)
            .elements()
            .iter()
            .find_map(|e| e.placeholder().or(e.value()).map(str::to_owned))
    })
    .unwrap()
}

// Maps is kept while another page shows, so its search boxes and Selects must take the new
// language when the player switches in Settings and comes back.
#[gpui_kit::test]
fn the_kept_maps_page_follows_a_language_switch(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::hold();
    let (env, server) = (support::env(), support::StoreServer::start());
    let s = support::maps::maps_shell(cx, &env, &server);
    let page = support::maps::open_maps(cx, &s);
    let _tab = support::maps::open_gamebanana(cx, &s, &page);
    let gb = [
        ids::MAPS_GB_SEARCH,
        ids::MAPS_GB_SORT,
        ids::MAPS_GB_GAMEMODE,
        ids::MAPS_GB_SHOW,
    ];
    let read = |cx: &mut TestAppContext, ids: &[&'static str]| -> Vec<Option<String>> {
        ids.iter().map(|id| shown_text(cx, s.win, id)).collect()
    };
    let some = |texts: &[&str]| -> Vec<Option<String>> {
        texts.iter().map(|t| Some((*t).to_owned())).collect()
    };
    assert_eq!(
        read(cx, &gb),
        some(&[
            "Search maps...",
            "Most downloaded",
            "All gamemodes",
            "All maps"
        ])
    );

    crate::settings::open_settings(cx, &s);
    crate::settings::pick(cx, s.win, "ru");
    support::click_id(cx, s.win, ids::nav(Page::Maps));
    assert_eq!(
        read(cx, &gb),
        some(&["Поиск карт...", "Популярные", "Все режимы", "Все карты"])
    );
    support::click_id(cx, s.win, ids::maps_source("serveme"));
    assert_eq!(
        read(cx, &[ids::MAPS_SEARCH, ids::MAPS_SORT]),
        some(&["Поиск карт...", "Название"])
    );
}

// The kept Maps page's failed serveme.tf fetch must be worded when it is drawn, not when it failed.
#[gpui_kit::test]
fn a_kept_maps_error_follows_a_language_switch(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::hold();
    let (env, server) = (support::env(), support::StoreServer::start());
    let s = support::maps::maps_shell(cx, &env, &server);
    server.reset();
    server.mount_status("/serveme/maps", 500);
    support::click_id(cx, s.win, ids::nav(Page::Maps));
    let page = cx
        .update(|cx| s.shell.read(cx).maps_page())
        .expect("a Maps page");
    support::wait_real(cx, "the failed fetch", |cx| {
        cx.update(|cx| page.read(cx).error().is_some() && !page.read(cx).is_syncing())
    });
    support::settle(cx, s.win);
    assert_eq!(
        support::label_of(cx, s.win, ids::MAPS_ERROR).as_deref(),
        Some("serveme.tf is having problems right now. Try again later.")
    );

    crate::settings::open_settings(cx, &s);
    crate::settings::pick(cx, s.win, "ru");
    support::click_id(cx, s.win, ids::nav(Page::Maps));
    support::click_id(cx, s.win, ids::maps_source("serveme"));
    assert_eq!(
        support::label_of(cx, s.win, ids::MAPS_ERROR).as_deref(),
        Some("На сайте serveme.tf сейчас неполадки. Попробуйте позже.")
    );
}

// A GameBanana map card's outdated badge and the Addon Store's install badges read in Russian.
#[gpui_kit::test]
fn the_card_badges_read_in_russian(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::switch("ru");
    let (env, server) = (support::env(), support::StoreServer::start());
    let s = support::maps::maps_shell_found(cx, &env, &server);
    support::maps::seed_maps(&env);
    support::sql(&env)
        .execute_batch(
            "UPDATE store_gb_maps SET obsolete = 1 WHERE gb_id = 10;
             UPDATE store_gb_maps SET mature = 1 WHERE gb_id = 11;",
        )
        .unwrap();
    cx.update(|cx| {
        let m = MatureAddons::global(cx);
        m.update(cx, |m, cx| m.set(true, cx))
    });
    let page = support::maps::open_maps(cx, &s);
    let _tab = support::maps::open_gamebanana(cx, &s, &page);
    assert_eq!(
        support::label_of(
            cx,
            s.win,
            ids::addon_gb_card_outdated("maps", GbId::new(10))
        )
        .as_deref(),
        Some("Устарело")
    );
    // The cover carries no accessible label; the scan of card.rs keeps its text keyed.
    assert!(support::present(
        cx,
        s.win,
        ids::addon_gb_card_reveal("maps", GbId::new(11))
    ));
    assert_eq!(
        support::label_of(cx, s.win, ids::addon_gb_card_author("maps", GbId::new(10))).as_deref(),
        Some("автор: vex")
    );
    let badge = |offer| CardBadge::of_offer(offer).map(|b| b.text.to_string());
    assert_eq!(badge(GbAddonOffer::Update).as_deref(), Some("Обновление"));
    assert_eq!(
        badge(GbAddonOffer::Installed).as_deref(),
        Some("Установлено")
    );
    assert_eq!(badge(GbAddonOffer::Install), None);
}

#[test]
fn map_texts_read_in_russian() {
    let _guard = LocaleGuard::switch("ru");
    let sorts: Vec<String> = SORTS
        .iter()
        .map(|(_, s)| sort_label(*s).to_string())
        .collect();
    assert_eq!(sorts, ["Название", "Чаще всего", "Недавние"]);
    assert_eq!(last_played_label(None), "Никогда");
    assert_eq!(
        maps_banner_text(true),
        "Синхронизация карт с GameBanana... Загружены ещё не все элементы."
    );
    assert_eq!(
        texts(&Confirm::remove_map("cp_alpha"))[..3],
        [
            "Удалить карту?",
            "cp_alpha будет удалена из TF2. Её можно скачать снова позже.",
            "Удалить"
        ]
    );
}

// A failed maps sync is kept by the app-wide MapsSync, so its line on the GameBanana tab must be
// worded when it is drawn and follow a switch made in Settings.
#[gpui_kit::test]
fn a_maps_sync_failure_follows_a_language_switch(cx: &mut TestAppContext) {
    let _guard = LocaleGuard::hold();
    let (env, server) = (support::env(), support::StoreServer::start());
    let s = support::maps::maps_shell(cx, &env, &server);
    let (sync, runs) = support::maps::fake_maps_sync(cx);
    let page = support::maps::open_maps(cx, &s);
    support::maps::open_gamebanana(cx, &s, &page);
    assert!(cx.update(|cx| sync.update(cx, |s, cx| s.start(false, cx))));
    cx.run_until_parked();
    let offline = SyncError::Network {
        url: "https://gamebanana.com/apiv11".into(),
        message: "connection refused".into(),
    };
    runs.try_recv()
        .unwrap()
        .finish(Err(describe_sync(StoreKind::GameBananaHuds, &offline)));
    support::wait_hub(cx, "the failure line", |cx| {
        support::settle(cx, s.win);
        support::present(cx, s.win, ids::MAPS_GB_ERROR)
    });
    assert_eq!(
        support::label_of(cx, s.win, ids::MAPS_GB_ERROR).as_deref(),
        Some("Couldn't reach GameBanana. Check your internet connection and try again.")
    );

    crate::settings::open_settings(cx, &s);
    crate::settings::pick(cx, s.win, "ru");
    support::click_id(cx, s.win, ids::nav(Page::Maps));
    support::settle(cx, s.win);
    assert_eq!(
        support::label_of(cx, s.win, ids::MAPS_GB_ERROR).as_deref(),
        Some(
            "Не удалось подключиться к GameBanana. Проверьте подключение к интернету и попробуйте снова."
        )
    );
}

#[test]
fn a_copied_hud_is_named_in_russian_and_portuguese() {
    let ru = {
        let _guard = LocaleGuard::switch("ru");
        [copy_name("flawhud", 1), copy_name("flawhud", 3)]
    };
    let pt = {
        let _guard = LocaleGuard::switch("pt-BR");
        [copy_name("flawhud", 1), copy_name("flawhud", 3)]
    };
    let en = {
        let _guard = LocaleGuard::switch("en");
        copy_name("flawhud", 2)
    };
    assert_eq!(ru, ["flawhud (копия)", "flawhud (копия 3)"]);
    assert_eq!(pt, ["flawhud (cópia)", "flawhud (cópia 3)"]);
    assert_eq!(en, "flawhud (copy 2)");
}
