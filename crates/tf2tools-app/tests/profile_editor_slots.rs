//! The draft editor's slots: rail, modes, and each Set body. Editing never writes the game.

use crate::support;

use gpui_kit::component::select::SelectEvent;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, TestAppContext, px, size};
use support::graphics::found;
use support::profiles::{
    FLAT, NULL, PYRO, addon, edit, game_files, low_fps60, page, seed, type_name,
};
use tf2tools_app::audio::player::{NowPlaying, sound_player};
use tf2tools_app::ids;
use tf2tools_app::loadout::BoardTarget;
use tf2tools_app::pages::profiles::draft_editor::BoardRef;
use tf2tools_app::pages::profiles::draft_editor::SoundOption;
use tf2tools_app::pages::profiles::editor_model::{Mode, SlotKind};
use tf2tools_app::services::OpError;
use tf2tools_app::widgets::loadout_board::BoardEvent;
use tf2tools_core::{
    Choices, CrosshairRef, DraftCrosshair, DraftItem, Preset, ProfileCrosshairRef, ProfileDraft,
    SlotMode, SoundKind, TfClass, WeaponSlot,
};

const SLOTS: [&str; 5] = ["hud", "sounds", "crosshairs", "graphics", "addons"];

fn comp(s: &support::ShellWindow, seeded: &support::profiles::Seeded) -> tf2tools_core::Profile {
    let draft = ProfileDraft::new("Comp")
        .with_hud(SlotMode::Set(DraftItem::Pick(seeded.rayshud.id().clone())))
        .with_sounds(SlotMode::Set((
            Some(DraftItem::Pick(seeded.ding.id().clone())),
            None,
        )))
        .with_crosshairs(SlotMode::None);
    s.services.library.create_profile(&draft).unwrap()
}

#[gpui_kit::test]
fn the_rail_badges_follow_each_slot_mode(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let seeded = seed(&s, &env);
    let p = comp(&s, &seeded);
    let page = page(cx, &s);
    let before = game_files(&env);
    edit(cx, &s, &page, p.id());
    let badges: Vec<String> = SLOTS
        .iter()
        .map(|slot| support::label_of(cx, s.win, ids::profile_rail_badge(slot)).unwrap())
        .collect();
    assert_eq!(badges, ["Set", "Set", "None", "Left as is", "Left as is"]);
    assert_eq!(
        support::selected_of(cx, s.win, ids::profile_slot_mode("crosshairs", "none")),
        Some(true)
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::profile_slot_text("crosshairs")).as_deref(),
        Some("Uses TF2's default crosshair on every weapon.")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::profile_slot_text("graphics")).as_deref(),
        Some("Applying this profile won't touch your graphics.")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::profile_slot_help("hud")).as_deref(),
        Some("The HUD this profile installs.")
    );
    assert!(
        !support::present(cx, s.win, ids::profile_slot_text("hud")),
        "Set shows its body"
    );
    assert_eq!(game_files(&env), before);
}

#[gpui_kit::test]
fn a_mode_click_changes_the_card_the_badge_and_dirty(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let seeded = seed(&s, &env);
    let p = comp(&s, &seeded);
    let page = page(cx, &s);
    let before = game_files(&env);
    let e = edit(cx, &s, &page, p.id());
    support::click_id(cx, s.win, ids::profile_slot_mode("hud", "none"));
    assert_eq!(
        support::label_of(cx, s.win, ids::profile_rail_badge("hud")).as_deref(),
        Some("None")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::profile_slot_text("hud")).as_deref(),
        Some("Uses TF2's own HUD: any custom HUD is taken out.")
    );
    assert!(cx.update(|cx| e.read(cx).is_dirty()));
    assert!(!support::disabled_of(cx, s.win, ids::PROFILE_EDITOR_SAVE));
    support::click_id(cx, s.win, ids::profile_slot_mode("hud", "leave"));
    assert_eq!(
        cx.update(|cx| e.read(cx).draft().mode(SlotKind::Hud)),
        Mode::Leave
    );
    support::click_id(cx, s.win, ids::profile_slot_mode("hud", "set"));
    assert!(
        !cx.update(|cx| e.read(cx).is_dirty()),
        "back to Set restores the saved HUD"
    );
    assert_eq!(game_files(&env), before);
}

#[gpui_kit::test]
fn saving_a_mode_change_stores_it(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let seeded = seed(&s, &env);
    let p = comp(&s, &seeded);
    let page = page(cx, &s);
    let before = game_files(&env);
    edit(cx, &s, &page, p.id());
    support::click_id(cx, s.win, ids::profile_slot_mode("sounds", "leave"));
    support::click_id(cx, s.win, ids::PROFILE_EDITOR_SAVE);
    support::wait_real(cx, "the list", |cx| {
        cx.update(|cx| page.read(cx).editor().is_none())
    });
    let saved = s.services.library.profile(p.id()).unwrap();
    assert!(matches!(saved.sounds(), SlotMode::Leave));
    assert!(saved.hud().as_set().is_some());
    assert_eq!(game_files(&env), before);
}

#[gpui_kit::test]
fn at_1280_the_rail_stands_beside_the_cards(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let seeded = seed(&s, &env);
    let p = comp(&s, &seeded);
    cx.simulate_window_resize(s.win, size(px(1280.), px(860.)));
    let page = page(cx, &s);
    let before = game_files(&env);
    edit(cx, &s, &page, p.id());
    let rail = support::bounds_of(cx, s.win, ids::PROFILE_RAIL).unwrap();
    assert!((rail.size.width - px(184.)).abs() <= px(1.), "{rail:?}");
    let items: Vec<_> = SLOTS
        .iter()
        .map(|k| support::bounds_of(cx, s.win, ids::profile_rail(k)).unwrap())
        .collect();
    assert!(
        items
            .windows(2)
            .all(|w| w[0].left() == w[1].left() && w[0].top() < w[1].top()),
        "{items:?}"
    );
    let card = support::bounds_of(cx, s.win, ids::profile_slot("hud")).unwrap();
    assert!(card.left() >= rail.right(), "{card:?} beside {rail:?}");
    assert_eq!(game_files(&env), before);
}

#[gpui_kit::test]
fn at_800x600_the_rail_is_a_row_and_nothing_overflows(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let seeded = seed(&s, &env);
    let p = comp(&s, &seeded);
    cx.simulate_window_resize(s.win, size(px(800.), px(600.)));
    let page = page(cx, &s);
    let before = game_files(&env);
    edit(cx, &s, &page, p.id());
    let body = support::bounds_of(cx, s.win, ids::PROFILE_EDITOR_BODY).unwrap();
    let rail = support::bounds_of(cx, s.win, ids::PROFILE_RAIL).unwrap();
    let first = support::bounds_of(cx, s.win, ids::profile_rail("hud")).unwrap();
    let second = support::bounds_of(cx, s.win, ids::profile_rail("sounds")).unwrap();
    assert_eq!(
        first.top(),
        second.top(),
        "the rail runs across: {first:?} {second:?}"
    );
    let card = support::bounds_of(cx, s.win, ids::profile_slot("hud")).unwrap();
    assert!(card.top() >= rail.bottom(), "the cards come below the rail");
    let mut checked = vec![
        ids::PROFILE_EDITOR_NAME.into(),
        ids::PROFILE_EDITOR_DELETE.into(),
    ];
    for slot in SLOTS {
        checked.push(ids::profile_rail(slot));
        checked.push(ids::profile_slot(slot));
        for mode in ["set", "none", "leave"] {
            checked.push(ids::profile_slot_mode(slot, mode));
        }
    }
    for id in checked {
        if let Some(b) = support::bounds_of(cx, s.win, id.clone()) {
            assert!(
                b.left() >= body.left() - px(1.) && b.right() <= body.right() + px(1.),
                "{id:?} {b:?} leaves {body:?}"
            );
        }
    }
    assert_eq!(game_files(&env), before);
}

#[gpui_kit::test]
fn picking_another_hud_marks_it_and_saves_it(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let seeded = seed(&s, &env);
    let p = comp(&s, &seeded);
    let page = page(cx, &s);
    let before = game_files(&env);
    let e = edit(cx, &s, &page, p.id());
    let (budhud, rayshud) = (seeded.budhud.folder_name(), seeded.rayshud.folder_name());
    support::wait_real(cx, "the HUD tiles", |cx| {
        support::present(cx, s.win, ids::profile_pick_hud(budhud))
    });
    let selected = |cx: &mut TestAppContext, folder: &str| {
        support::selected_of(cx, s.win, ids::profile_pick_hud(folder)) == Some(true)
    };
    assert!(selected(cx, rayshud));
    support::click_id(cx, s.win, ids::profile_pick_hud(budhud));
    assert!(selected(cx, budhud));
    assert!(!selected(cx, rayshud));
    assert!(cx.update(|cx| e.read(cx).is_dirty()));
    support::click_id(cx, s.win, ids::PROFILE_EDITOR_SAVE);
    support::wait_real(cx, "the list", |cx| {
        cx.update(|cx| page.read(cx).editor().is_none())
    });
    let saved = s.services.library.profile(p.id()).unwrap();
    assert_eq!(saved.hud().as_set().map(|h| h.folder_name()), Some(budhud));
    assert_eq!(game_files(&env), before);
}

#[gpui_kit::test]
fn the_sound_selects_pick_a_sound_or_tf2_default(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let seeded = seed(&s, &env);
    let p = comp(&s, &seeded);
    let page = page(cx, &s);
    let before = game_files(&env);
    let e = edit(cx, &s, &page, p.id());
    support::wait_real(cx, "the sound rows", |cx| {
        support::present(cx, s.win, ids::PROFILE_SOUND_KILL)
    });
    assert!(
        support::disabled_of(cx, s.win, ids::PROFILE_SOUND_KILL_PLAY),
        "TF2 default has nothing to play"
    );
    let kill = cx.update(|cx| e.read(cx).sound_select(SoundKind::Killsound).clone());
    let bell = SoundOption {
        id: Some(seeded.bell.id().clone()),
        label: "Bell".into(),
    };
    cx.update(|cx| {
        kill.update(cx, |_, cx| {
            cx.emit(SelectEvent::<Vec<SoundOption>>::Confirm(Some(bell)))
        })
    });
    support::settle(cx, s.win);
    let hit = cx.update(|cx| e.read(cx).sound_select(SoundKind::Hitsound).clone());
    let default = SoundOption {
        id: None,
        label: "TF2 default".into(),
    };
    cx.update(|cx| {
        hit.update(cx, |_, cx| {
            cx.emit(SelectEvent::<Vec<SoundOption>>::Confirm(Some(default)))
        })
    });
    support::settle(cx, s.win);
    assert!(!support::disabled_of(
        cx,
        s.win,
        ids::PROFILE_SOUND_KILL_PLAY
    ));
    support::click_id(cx, s.win, ids::profile_slot_mode("sounds", "set"));
    support::click_id(cx, s.win, ids::PROFILE_EDITOR_SAVE);
    support::wait_real(cx, "the list", |cx| {
        cx.update(|cx| page.read(cx).editor().is_none())
    });
    let saved = s.services.library.profile(p.id()).unwrap();
    let sounds = saved.sounds().as_set().expect("Sounds stays Set");
    assert!(sounds.hit.is_none());
    assert_eq!(
        sounds.kill.as_ref().map(|k| k.folder_name()),
        Some(seeded.bell.folder_name())
    );
    assert_eq!(game_files(&env), before);
}

#[gpui_kit::test]
fn play_previews_the_picked_sound(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let seeded = seed(&s, &env);
    let p = comp(&s, &seeded);
    let page = page(cx, &s);
    edit(cx, &s, &page, p.id());
    support::wait_real(cx, "the sound rows", |cx| {
        support::present(cx, s.win, ids::PROFILE_SOUND_HIT_PLAY)
    });
    support::click_id(cx, s.win, ids::PROFILE_SOUND_HIT_PLAY);
    support::wait_real(cx, "the play", |_| s.audio.plays().len() == 1);
}

#[gpui_kit::test]
fn an_empty_hud_library_says_so(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let p = s
        .services
        .library
        .create_profile(&ProfileDraft::new("Bare"))
        .unwrap();
    let page = page(cx, &s);
    let e = edit(cx, &s, &page, p.id());
    cx.update(|cx| e.update(cx, |e, cx| e.set_mode(SlotKind::Hud, Mode::Set, cx)));
    support::wait_real(cx, "the empty line", |cx| {
        support::present(cx, s.win, ids::PROFILE_HUD_EMPTY)
    });
    assert_eq!(
        support::label_of(cx, s.win, ids::PROFILE_HUD_EMPTY).as_deref(),
        Some("No HUDs in your library yet.")
    );
}

#[gpui_kit::test]
fn a_kept_hud_and_sound_missing_from_the_library_still_show(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let seeded = seed(&s, &env);
    let p = comp(&s, &seeded);
    s.services.library.delete_hud(seeded.rayshud.id()).unwrap();
    let page = page(cx, &s);
    let e = edit(cx, &s, &page, p.id());
    let folder = seeded.rayshud.folder_name().to_owned();
    support::wait_real(cx, "the kept tile", |cx| {
        support::present(cx, s.win, ids::profile_pick_hud(&folder))
    });
    assert_eq!(
        support::selected_of(cx, s.win, ids::profile_pick_hud(&folder)),
        Some(true)
    );
    let shown = support::label_of(cx, s.win, ids::profile_pick_hud(&folder)).unwrap();
    assert!(shown.contains("rayshud"), "{shown}");
    assert!(
        !cx.update(|cx| e.read(cx).is_dirty()),
        "a kept item is not an edit"
    );
    support::click_id(cx, s.win, ids::profile_slot_mode("hud", "none"));
    support::click_id(cx, s.win, ids::profile_slot_mode("hud", "set"));
    assert_eq!(
        support::selected_of(cx, s.win, ids::profile_pick_hud(&folder)),
        Some(true),
        "the kept HUD survives a mode switch"
    );
}

#[gpui_kit::test]
fn saving_a_hud_deleted_meanwhile_toasts_and_drops_its_tile(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let seeded = seed(&s, &env);
    let p = comp(&s, &seeded);
    let page = page(cx, &s);
    edit(cx, &s, &page, p.id());
    let budhud = seeded.budhud.folder_name().to_owned();
    support::wait_real(cx, "the tiles", |cx| {
        support::present(cx, s.win, ids::profile_pick_hud(&budhud))
    });
    support::click_id(cx, s.win, ids::profile_pick_hud(&budhud));
    s.services.library.delete_hud(seeded.budhud.id()).unwrap();
    support::click_id(cx, s.win, ids::PROFILE_EDITOR_SAVE);
    support::wait_real(cx, "the toast", |cx| {
        support::present(cx, s.win, ids::TOAST)
    });
    assert!(
        cx.update(|cx| page.read(cx).editor().is_some()),
        "the draft stays open"
    );
    support::wait_real(cx, "the reload", |cx| {
        !support::present(cx, s.win, ids::profile_pick_hud(&budhud))
    });
    let saved = s.services.library.profile(p.id()).unwrap();
    assert_eq!(
        saved.hud().as_set().map(|h| h.folder_name()),
        Some(seeded.rayshud.folder_name())
    );
}

#[gpui_kit::test]
fn a_refused_save_drops_the_deleted_picks_so_the_next_save_works(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let seeded = seed(&s, &env);
    let p = comp(&s, &seeded);
    let page = page(cx, &s);
    let before = game_files(&env);
    let e = edit(cx, &s, &page, p.id());
    type_name(cx, &s, &e, "Comp two");
    let budhud = seeded.budhud.folder_name().to_owned();
    support::wait_real(cx, "the tiles", |cx| {
        support::present(cx, s.win, ids::profile_pick_hud(&budhud))
    });
    support::click_id(cx, s.win, ids::profile_pick_hud(&budhud));
    let kill = cx.update(|cx| e.read(cx).sound_select(SoundKind::Killsound).clone());
    let bell = SoundOption {
        id: Some(seeded.bell.id().clone()),
        label: "Bell".into(),
    };
    cx.update(|cx| {
        kill.update(cx, |_, cx| {
            cx.emit(SelectEvent::<Vec<SoundOption>>::Confirm(Some(bell)))
        })
    });
    s.services.library.delete_hud(seeded.budhud.id()).unwrap();
    s.services
        .library
        .delete_sound(seeded.bell.id(), None)
        .unwrap();
    support::click_id(cx, s.win, ids::PROFILE_EDITOR_SAVE);
    support::wait_real(cx, "the reload", |cx| {
        !support::present(cx, s.win, ids::profile_pick_hud(&budhud))
    });
    let draft = cx.update(|cx| e.read(cx).draft().clone());
    assert!(draft.hud.is_none(), "the deleted HUD left the draft");
    assert!(draft.kill.is_none(), "the deleted killsound left the draft");
    assert_eq!(
        support::label_of(cx, s.win, ids::PROFILE_EDITOR_NAME_ERROR).as_deref(),
        Some("Pick a HUD, or choose None")
    );
    support::click_id(
        cx,
        s.win,
        ids::profile_pick_hud(seeded.rayshud.folder_name()),
    );
    support::click_id(cx, s.win, ids::PROFILE_EDITOR_SAVE);
    support::wait_real(cx, "the list", |cx| {
        cx.update(|cx| page.read(cx).editor().is_none())
    });
    let saved = s.services.library.profile(p.id()).unwrap();
    assert_eq!(saved.name(), "Comp two");
    let sounds = saved.sounds().as_set().expect("Sounds stays Set");
    assert!(sounds.kill.is_none());
    assert_eq!(game_files(&env), before);
}

type Load = Result<Vec<u8>, OpError>;

/// Holds the shared player on `folder`'s sound with a load that resolves only when the test
/// sends through the returned gate, as an editor play button's load would while its bytes are
/// read.
fn hold_load(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    folder: &str,
) -> async_channel::Sender<Load> {
    let (tx, rx) = async_channel::bounded::<Load>(1);
    let player = cx
        .update_window(s.win, |_, w, cx| sound_player(w, cx))
        .unwrap();
    let now = NowPlaying {
        key: format!("sound:{folder}").into(),
        name: "Ding".into(),
        subtitle: "test".into(),
    };
    cx.update(|cx| {
        player.update(cx, |p, cx| {
            p.toggle_with(
                now,
                move |cx| {
                    cx.background_spawn(async move {
                        rx.recv()
                            .await
                            .unwrap_or_else(|_| Err(OpError::core("load".into(), "gate dropped")))
                    })
                },
                cx,
            )
            .detach()
        })
    });
    support::settle(cx, s.win);
    tx
}

#[gpui_kit::test]
fn a_sound_play_button_spins_and_reads_cancel_while_loading(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let seeded = seed(&s, &env);
    let p = comp(&s, &seeded);
    let page = page(cx, &s);
    edit(cx, &s, &page, p.id());
    support::wait_real(cx, "the sound rows", |cx| {
        support::present(cx, s.win, ids::PROFILE_SOUND_HIT_PLAY)
    });
    let gate = hold_load(cx, &s, seeded.ding.folder_name());
    assert!(support::present(
        cx,
        s.win,
        ids::PROFILE_SOUND_HIT_PLAY_SPINNER
    ));
    assert_eq!(
        support::label_of(cx, s.win, ids::PROFILE_SOUND_HIT_PLAY).as_deref(),
        Some("Cancel")
    );
    let button = support::bounds_of(cx, s.win, ids::PROFILE_SOUND_HIT_PLAY).unwrap();
    assert_eq!(button.size, size(px(32.), px(32.)), "{button:?}");
    support::click_id(cx, s.win, ids::PROFILE_SOUND_HIT_PLAY);
    let player = cx
        .update_window(s.win, |_, w, cx| sound_player(w, cx))
        .unwrap();
    assert!(cx.update(|cx| player.read(cx).now_playing().is_none()));
    gate.send_blocking(Ok(support::wav_bytes())).unwrap();
    support::settle(cx, s.win);
    assert!(s.audio.plays().is_empty(), "a cancelled load never plays");
    assert_eq!(
        support::label_of(cx, s.win, ids::PROFILE_SOUND_HIT_PLAY).as_deref(),
        Some("Play")
    );
}

#[gpui_kit::test]
fn a_playing_sound_reads_stop(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let seeded = seed(&s, &env);
    let p = comp(&s, &seeded);
    let page = page(cx, &s);
    edit(cx, &s, &page, p.id());
    support::wait_real(cx, "the sound rows", |cx| {
        support::present(cx, s.win, ids::PROFILE_SOUND_HIT_PLAY)
    });
    support::click_id(cx, s.win, ids::PROFILE_SOUND_HIT_PLAY);
    support::wait_real(cx, "the play", |_| s.audio.plays().len() == 1);
    assert_eq!(
        support::label_of(cx, s.win, ids::PROFILE_SOUND_HIT_PLAY).as_deref(),
        Some("Stop")
    );
    let button = support::bounds_of(cx, s.win, ids::PROFILE_SOUND_HIT_PLAY).unwrap();
    assert_eq!(button.size, size(px(32.), px(32.)), "{button:?}");
    support::click_id(cx, s.win, ids::PROFILE_SOUND_HIT_PLAY);
    let player = cx
        .update_window(s.win, |_, w, cx| sound_player(w, cx))
        .unwrap();
    assert!(cx.update(|cx| player.read(cx).now_playing().is_none()));
    assert_eq!(
        support::label_of(cx, s.win, ids::PROFILE_SOUND_HIT_PLAY).as_deref(),
        Some("Play")
    );
}

fn xh_profile(s: &support::ShellWindow) -> tf2tools_core::Profile {
    let draft = ProfileDraft::new("Aim").with_crosshairs(SlotMode::Set(Vec::new()));
    s.services.library.create_profile(&draft).unwrap()
}

#[gpui_kit::test]
fn a_board_click_puts_the_picked_crosshair_on_that_weapon(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let p = xh_profile(&s);
    let page = page(cx, &s);
    let before = game_files(&env);
    let e = edit(cx, &s, &page, p.id());
    let xh = tf2tools_app::ids::IdScope::PROFILE_CROSSHAIRS;
    assert!(support::present(cx, s.win, xh.id(ids::CROSSHAIRS_BOARD)));
    assert!(support::disabled_of(cx, s.win, ids::PROFILE_XH_EVERYWHERE));
    let dot = CrosshairRef::Builtin(s.services.builtins.all()[0].name().to_owned());
    let picker = cx.update(|cx| e.read(cx).picker().clone());
    cx.update(|cx| picker.update(cx, |p, cx| p.select(Some(dot.clone()), cx)));
    support::settle(cx, s.win);
    // The board sits below the fold at the test window's size, so jump to its card first.
    support::click_id(cx, s.win, ids::profile_rail("crosshairs"));
    support::click_id(
        cx,
        s.win,
        xh.id(ids::loadout_cell(TfClass::Scout, WeaponSlot::Primary)),
    );
    let shown = cx.update(|cx| e.read(cx).draft().crosshairs.clone());
    assert!(!shown.is_empty(), "the scout's primaries took it");
    let label = support::label_of(cx, s.win, ids::PROFILE_XH_SELECTION).unwrap();
    assert!(
        label.contains(&format!("On {} weapon", shown.len())),
        "{label}"
    );
    support::click_id(cx, s.win, ids::PROFILE_EDITOR_SAVE);
    support::wait_real(cx, "the list", |cx| {
        cx.update(|cx| page.read(cx).editor().is_none())
    });
    let saved = s.services.library.profile(p.id()).unwrap();
    assert_eq!(saved.crosshairs().as_set().map(Vec::len), Some(shown.len()));
    assert_eq!(game_files(&env), before);
}

#[gpui_kit::test]
fn put_on_every_weapon_covers_the_whole_board(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let p = xh_profile(&s);
    let page = page(cx, &s);
    let e = edit(cx, &s, &page, p.id());
    let dot = CrosshairRef::Builtin(s.services.builtins.all()[0].name().to_owned());
    let picker = cx.update(|cx| e.read(cx).picker().clone());
    cx.update(|cx| picker.update(cx, |p, cx| p.select(Some(dot), cx)));
    support::settle(cx, s.win);
    support::click_id(cx, s.win, ids::PROFILE_XH_EVERYWHERE);
    let every = BoardTarget::Everything.scripts().len();
    assert_eq!(cx.update(|cx| e.read(cx).draft().crosshairs.len()), every);
}

#[gpui_kit::test]
fn other_board_events_change_nothing(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let p = xh_profile(&s);
    let page = page(cx, &s);
    let e = edit(cx, &s, &page, p.id());
    for event in [
        BoardEvent::Open(TfClass::Pyro, WeaponSlot::Secondary),
        BoardEvent::Close,
    ] {
        cx.update_window(s.win, |_, window, cx| {
            e.update(cx, |e, cx| e.board_event(event, window, cx))
        })
        .unwrap();
    }
    assert!(!cx.update(|cx| e.read(cx).is_dirty()));
}

#[gpui_kit::test]
fn add_to_profile_from_crosshairs_sets_the_slot(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let p = s
        .services
        .library
        .create_profile(&ProfileDraft::new("Bare"))
        .unwrap();
    let page = page(cx, &s);
    let dot = CrosshairRef::Builtin(s.services.builtins.all()[0].name().to_owned());
    let id = p.id().clone();
    cx.update_window(s.win, |_, window, cx| {
        page.update(cx, |pg, cx| {
            pg.open_with_crosshair(id, dot.clone(), window, cx)
        })
    })
    .unwrap();
    support::wait_real(cx, "the editor", |cx| {
        cx.update(|cx| page.read(cx).editor().is_some())
    });
    support::settle(cx, s.win);
    let e = cx.update(|cx| page.read(cx).editor()).unwrap();
    assert_eq!(
        cx.update(|cx| e.read(cx).draft().mode(SlotKind::Crosshairs)),
        Mode::Set
    );
    assert!(cx.update(|cx| e.read(cx).is_dirty()));
    assert_eq!(
        cx.update(|cx| e.read(cx).picker().read(cx).selected().cloned()),
        Some(dot)
    );
}

fn library_xh_profile(
    s: &support::ShellWindow,
    env: &support::Env,
) -> (tf2tools_core::Profile, tf2tools_core::CrosshairEntry) {
    let entry = support::seed_crosshair(&s.services, env, "mine");
    let set = DraftCrosshair::Set {
        weapon_script: "tf_weapon_scattergun".into(),
        crosshair: CrosshairRef::Library(entry.id().clone()),
    };
    let p = s
        .services
        .library
        .create_profile(&ProfileDraft::new("Mine").with_crosshairs(SlotMode::Set(vec![set])))
        .unwrap();
    (p, entry)
}

#[gpui_kit::test]
fn a_saved_library_crosshair_shows_on_the_board(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let (p, entry) = library_xh_profile(&s, &env);
    let page = page(cx, &s);
    let e = edit(cx, &s, &page, p.id());
    let early = cx.update(|cx| e.read(cx).board_ref("tf_weapon_scattergun"));
    assert!(
        !matches!(early, Some(BoardRef::Waiting(_))),
        "never waiting before the library loads: {early:?}"
    );
    support::wait_real(cx, "the mapping", |cx| {
        cx.update(|cx| e.read(cx).board_ref("tf_weapon_scattergun").is_some())
    });
    assert_eq!(
        cx.update(|cx| e.read(cx).board_ref("tf_weapon_scattergun")),
        Some(BoardRef::Shown(CrosshairRef::Library(entry.id().clone())))
    );
    assert_eq!(
        cx.update(|cx| e.read(cx).board_ref("tf_weapon_pistol")),
        None
    );
}

#[gpui_kit::test]
fn a_deleted_library_crosshair_waits_and_survives_save(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let (p, entry) = library_xh_profile(&s, &env);
    s.services.library.delete_crosshair(entry.id()).unwrap();
    let page = page(cx, &s);
    let e = edit(cx, &s, &page, p.id());
    support::wait_real(cx, "the mapping", |cx| {
        cx.update(|cx| e.read(cx).board_ref("tf_weapon_scattergun").is_some())
    });
    assert_eq!(
        cx.update(|cx| e.read(cx).board_ref("tf_weapon_scattergun")),
        Some(BoardRef::Waiting(entry.folder_name().to_owned()))
    );
    type_name(cx, &s, &e, "Mine 2");
    support::click_id(cx, s.win, ids::PROFILE_EDITOR_SAVE);
    support::wait_real(cx, "the list", |cx| {
        cx.update(|cx| page.read(cx).editor().is_none())
    });
    let saved = s.services.library.profile(p.id()).unwrap();
    let kept = saved.crosshairs().as_set().unwrap();
    assert_eq!(
        kept[0].crosshair(),
        &ProfileCrosshairRef::Library {
            folder_name: entry.folder_name().to_owned()
        },
        "the kept entry is saved unchanged"
    );
}

#[gpui_kit::test]
fn assigning_a_library_crosshair_saves_its_folder(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let entry = support::seed_crosshair(&s.services, &env, "mine");
    let p = xh_profile(&s);
    let page = page(cx, &s);
    let e = edit(cx, &s, &page, p.id());
    let picker = cx.update(|cx| e.read(cx).picker().clone());
    cx.update(|cx| {
        picker.update(cx, |p, cx| {
            p.select(Some(CrosshairRef::Library(entry.id().clone())), cx)
        })
    });
    support::settle(cx, s.win);
    let xh = tf2tools_app::ids::IdScope::PROFILE_CROSSHAIRS;
    // The board sits below the fold at the test window's size, so jump to its card first.
    support::click_id(cx, s.win, ids::profile_rail("crosshairs"));
    support::click_id(
        cx,
        s.win,
        xh.id(ids::loadout_cell(TfClass::Sniper, WeaponSlot::Melee)),
    );
    support::click_id(cx, s.win, ids::PROFILE_EDITOR_SAVE);
    support::wait_real(cx, "the list", |cx| {
        cx.update(|cx| page.read(cx).editor().is_none())
    });
    let saved = s.services.library.profile(p.id()).unwrap();
    let set = saved.crosshairs().as_set().unwrap();
    assert!(!set.is_empty());
    assert!(set.iter().all(|c| c.crosshair()
        == &ProfileCrosshairRef::Library {
            folder_name: entry.folder_name().to_owned()
        }));
}

#[gpui_kit::test]
fn at_1280x800_the_crosshairs_card_holds_its_strip_and_board(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let p = xh_profile(&s);
    cx.simulate_window_resize(s.win, size(px(1280.), px(800.)));
    let page = page(cx, &s);
    edit(cx, &s, &page, p.id());
    let xh = tf2tools_app::ids::IdScope::PROFILE_CROSSHAIRS;
    let card = support::bounds_of(cx, s.win, ids::profile_slot("crosshairs")).unwrap();
    for id in [
        xh.id(ids::CROSSHAIRS_BOARD),
        ids::PROFILE_XH_SELECTION.into(),
        ids::PROFILE_XH_EVERYWHERE.into(),
    ] {
        let b = support::bounds_of(cx, s.win, id.clone()).unwrap();
        assert!(
            b.left() >= card.left() && b.right() <= card.right() + px(1.),
            "{id:?} {b:?} leaves {card:?}"
        );
    }
    let board = support::bounds_of(cx, s.win, xh.id(ids::CROSSHAIRS_BOARD)).unwrap();
    assert!(
        board.size.width >= card.size.width * 0.8,
        "the board fills the card: {board:?} in {card:?}"
    );
}

#[gpui_kit::test]
fn the_strips_import_adds_the_crosshair_and_picks_it(cx: &mut TestAppContext) {
    let env = support::env();
    let path = env.tmp.path().join("Ring.png");
    std::fs::write(&path, support::crosshair_png([255, 0, 0, 255])).unwrap();
    let files = std::sync::Arc::new(tf2tools_app::picker::ScriptedPicker::new([path]));
    let s = support::shell_window_with(cx, &env, found(&env), files, support::no_steam());
    let p = xh_profile(&s);
    let page = page(cx, &s);
    let before = game_files(&env);
    let e = edit(cx, &s, &page, p.id());
    let xh = tf2tools_app::ids::IdScope::PROFILE_CROSSHAIRS;
    support::click_id(cx, s.win, xh.id(ids::CROSSHAIRS_IMPORT));
    support::wait_real(cx, "the toast", |cx| {
        support::present(cx, s.win, ids::TOAST)
    });
    let entries = s.services.library.list_crosshairs().unwrap();
    assert_eq!(entries.len(), 1);
    let ring = CrosshairRef::Library(entries[0].id().clone());
    assert_eq!(
        cx.update(|cx| e.read(cx).picker().read(cx).selected().cloned()),
        Some(ring.clone())
    );
    support::wait_real(cx, "the library to list it", |cx| {
        cx.update(|cx| e.read(cx).picker().read(cx).name_of(&ring).is_some())
    });
    // The success toast sits over the button, so assign as Put on every weapon does.
    cx.update(|cx| e.update(cx, |e, cx| e.assign(BoardTarget::Everything, cx)));
    assert_eq!(
        cx.update(|cx| e.read(cx).board_ref("tf_weapon_scattergun")),
        Some(BoardRef::Shown(ring))
    );
    assert_eq!(game_files(&env), before);
}

#[gpui_kit::test]
fn dropping_a_strip_tile_on_a_cell_puts_it_on_that_weapon(cx: &mut TestAppContext) {
    use gpui_kit::test::TestWindowExt as _;
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let p = xh_profile(&s);
    let page = page(cx, &s);
    let before = game_files(&env);
    let e = edit(cx, &s, &page, p.id());
    let xh = tf2tools_app::ids::IdScope::PROFILE_CROSSHAIRS;
    let first = s.services.builtins.all()[0].name().to_owned();
    support::click_id(cx, s.win, ids::profile_rail("crosshairs"));
    let tile = xh.id(ids::crosshair_builtin(&first));
    let cell = xh.id(ids::loadout_cell(TfClass::Scout, WeaponSlot::Primary));
    cx.update_window(s.win, |_, w, cx| w.drag_to(tile, cell, cx))
        .unwrap();
    support::settle(cx, s.win);
    let scripts = BoardTarget::Cell(TfClass::Scout, WeaponSlot::Primary).scripts();
    for script in &scripts {
        assert_eq!(
            cx.update(|cx| e.read(cx).board_ref(script)),
            Some(BoardRef::Shown(CrosshairRef::Builtin(first.clone()))),
            "{script}"
        );
    }
    assert_eq!(
        cx.update(|cx| e.read(cx).draft().crosshairs.len()),
        scripts.len()
    );
    assert_eq!(game_files(&env), before);
}

fn gfx_profile(s: &support::ShellWindow, env: &support::Env) -> tf2tools_core::Profile {
    let vpk = support::vpk_bytes(&[("cfg/addon.cfg", b"echo addon\n")]);
    for name in [PYRO, FLAT] {
        support::seed_library_addon(&s.services, env, name, "9.100.1", &vpk, false);
    }
    let draft = ProfileDraft::new("Potato")
        .with_graphics(SlotMode::Set(low_fps60()))
        .with_addons(SlotMode::Set((vec![DraftItem::Pick(addon(PYRO))], true)));
    s.services.library.create_profile(&draft).unwrap()
}

#[gpui_kit::test]
fn graphics_shows_the_presets_rows_and_changes_under_profile_ids(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    support::seed_managed_mastercomfig(&env, "9.100.1", "medium");
    support::seed_mastercomfig_catalog(&env, "9.100.1");
    let p = gfx_profile(&s, &env);
    let page = page(cx, &s);
    let before = game_files(&env);
    let e = edit(cx, &s, &page, p.id());
    let gfx = tf2tools_app::ids::IdScope::PROFILE_GRAPHICS;
    support::wait_real(cx, "the rows", |cx| {
        support::present(cx, s.win, gfx.id(ids::graphics_row("fpscap")))
    });
    assert_eq!(
        support::checked_of(cx, s.win, gfx.id(ids::graphics_preset(Preset::Low))),
        Some(true)
    );
    assert!(support::present(
        cx,
        s.win,
        gfx.id(ids::graphics_change("fpscap"))
    ));
    assert!(
        !support::present(cx, s.win, ids::graphics_row("fpscap")),
        "no Graphics page ids"
    );
    support::click_id(cx, s.win, ids::profile_rail("graphics"));
    support::click_id(cx, s.win, gfx.id(ids::graphics_preset(Preset::High)));
    let choices = cx.update(|cx| e.read(cx).draft().graphics.clone());
    assert_eq!(choices.preset(), Preset::High);
    assert!(
        choices.modules().contains_key("fpscap"),
        "the change survives a preset switch"
    );
    assert_eq!(game_files(&env), before);
}

#[gpui_kit::test]
fn without_a_catalog_only_the_presets_show(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let p = gfx_profile(&s, &env);
    let page = page(cx, &s);
    let before = game_files(&env);
    edit(cx, &s, &page, p.id());
    let gfx = tf2tools_app::ids::IdScope::PROFILE_GRAPHICS;
    support::wait_real(cx, "the no-catalog line", |cx| {
        support::present(cx, s.win, ids::PROFILE_GFX_NO_CATALOG)
    });
    assert!(support::present(
        cx,
        s.win,
        gfx.id(ids::graphics_preset(Preset::Low))
    ));
    assert_eq!(
        support::label_of(cx, s.win, ids::PROFILE_GFX_NO_CATALOG).as_deref(),
        Some("Open the Graphics page once to choose settings")
    );
    assert!(!support::present(
        cx,
        s.win,
        gfx.id(ids::graphics_row("fpscap"))
    ));
    assert_eq!(game_files(&env), before);
}

#[gpui_kit::test]
fn copy_from_the_graphics_page_takes_the_saved_choices(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    support::seed_managed_mastercomfig(&env, "9.100.1", "ultra");
    support::seed_mastercomfig_catalog(&env, "9.100.1");
    support::seed_mastercomfig_choices(&env, r#"{"fpscap":"144"}"#);
    let p = gfx_profile(&s, &env);
    let page = page(cx, &s);
    let before = game_files(&env);
    let e = edit(cx, &s, &page, p.id());
    support::wait_real(cx, "the saved choices", |cx| {
        support::present(cx, s.win, ids::PROFILE_GFX_COPY)
            && !support::disabled_of(cx, s.win, ids::PROFILE_GFX_COPY)
    });
    support::click_id(cx, s.win, ids::profile_rail("graphics"));
    support::click_id(cx, s.win, ids::PROFILE_GFX_COPY);
    support::wait_real(cx, "the copy", |cx| {
        cx.update(|cx| e.read(cx).draft().graphics.preset() == Preset::Ultra)
    });
    assert_ne!(
        cx.update(|cx| e.read(cx).draft().graphics.clone()),
        Choices::new(Preset::Ultra)
    );
    assert_eq!(game_files(&env), before);
}

#[gpui_kit::test]
fn copy_is_disabled_without_a_managed_mastercomfig(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let p = gfx_profile(&s, &env);
    let page = page(cx, &s);
    let before = game_files(&env);
    edit(cx, &s, &page, p.id());
    support::wait_real(cx, "the copy button", |cx| {
        support::present(cx, s.win, ids::PROFILE_GFX_COPY)
    });
    support::settle(cx, s.win);
    assert!(support::disabled_of(cx, s.win, ids::PROFILE_GFX_COPY));
    assert_eq!(game_files(&env), before);
}

#[gpui_kit::test]
fn a_preset_switch_without_a_catalog_keeps_the_module_changes(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let p = gfx_profile(&s, &env);
    let page = page(cx, &s);
    let before = game_files(&env);
    let e = edit(cx, &s, &page, p.id());
    let gfx = tf2tools_app::ids::IdScope::PROFILE_GRAPHICS;
    support::click_id(cx, s.win, ids::profile_rail("graphics"));
    support::click_id(cx, s.win, gfx.id(ids::graphics_preset(Preset::High)));
    let choices = cx.update(|cx| e.read(cx).draft().graphics.clone());
    assert_eq!(choices.preset(), Preset::High);
    assert_eq!(
        choices.modules(),
        low_fps60().modules(),
        "no catalog, and still nothing is lost"
    );
    assert_eq!(game_files(&env), before);
}

#[gpui_kit::test]
fn the_addon_switches_and_only_these_edit_the_draft(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let p = gfx_profile(&s, &env);
    let page = page(cx, &s);
    let before = game_files(&env);
    let e = edit(cx, &s, &page, p.id());
    support::wait_real(cx, "the switches", |cx| {
        support::present(cx, s.win, ids::profile_addon(FLAT))
    });
    assert_eq!(
        support::label_of(cx, s.win, ids::profile_addon(FLAT)).as_deref(),
        Some("Flat Mouse")
    );
    assert_eq!(
        support::checked_of(cx, s.win, ids::profile_addon(PYRO)),
        Some(true)
    );
    assert_eq!(
        support::checked_of(cx, s.win, ids::profile_addon(FLAT)),
        Some(false)
    );
    assert_eq!(
        support::checked_of(cx, s.win, ids::PROFILE_ADDONS_ONLY),
        Some(true)
    );
    support::click_id(cx, s.win, ids::profile_rail("addons"));
    support::click_id(cx, s.win, ids::profile_addon(FLAT));
    support::click_id(cx, s.win, ids::PROFILE_ADDONS_ONLY);
    support::click_id(cx, s.win, ids::PROFILE_EDITOR_SAVE);
    support::wait_real(cx, "the list", |cx| {
        cx.update(|cx| page.read(cx).editor().is_none())
    });
    let saved = s.services.library.profile(p.id()).unwrap();
    let addons = saved.addons().as_set().expect("Set");
    assert_eq!(addons.on, [addon(FLAT), addon(PYRO)]);
    assert!(!addons.only_these);
    drop(e);
    assert_eq!(game_files(&env), before);
}

#[gpui_kit::test]
fn addons_set_is_disabled_while_graphics_is_none(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let p = gfx_profile(&s, &env);
    let page = page(cx, &s);
    let before = game_files(&env);
    let e = edit(cx, &s, &page, p.id());
    support::click_id(cx, s.win, ids::profile_rail("graphics"));
    support::click_id(cx, s.win, ids::profile_slot_mode("graphics", "none"));
    assert_eq!(
        cx.update(|cx| e.read(cx).draft().mode(SlotKind::Addons)),
        Mode::None
    );
    assert!(support::disabled_of(
        cx,
        s.win,
        ids::profile_slot_mode("addons", "set")
    ));
    assert_eq!(
        support::label_of(cx, s.win, ids::PROFILE_ADDONS_NEED_GRAPHICS).as_deref(),
        Some("Add-ons need mastercomfig, and Graphics is set to None")
    );
    support::click_id(cx, s.win, ids::profile_rail("addons"));
    support::click_id(cx, s.win, ids::profile_slot_mode("addons", "set"));
    assert_eq!(
        cx.update(|cx| e.read(cx).draft().mode(SlotKind::Addons)),
        Mode::None
    );
    support::click_id(cx, s.win, ids::profile_rail("graphics"));
    support::click_id(cx, s.win, ids::profile_slot_mode("graphics", "leave"));
    assert!(!support::disabled_of(
        cx,
        s.win,
        ids::profile_slot_mode("addons", "set")
    ));
    assert!(!support::present(
        cx,
        s.win,
        ids::PROFILE_ADDONS_NEED_GRAPHICS
    ));
    assert_eq!(game_files(&env), before);
}

#[gpui_kit::test]
fn an_empty_addon_library_points_at_the_store(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let p = s
        .services
        .library
        .create_profile(
            &ProfileDraft::new("None yet").with_addons(SlotMode::Set((Vec::new(), false))),
        )
        .unwrap();
    let page = page(cx, &s);
    let before = game_files(&env);
    edit(cx, &s, &page, p.id());
    support::wait_real(cx, "the empty line", |cx| {
        support::present(cx, s.win, ids::PROFILE_ADDONS_EMPTY)
    });
    assert_eq!(
        support::label_of(cx, s.win, ids::PROFILE_ADDONS_EMPTY).as_deref(),
        Some("No mastercomfig add-ons in your library yet.")
    );
    assert_eq!(
        support::label_of(cx, s.win, ids::PROFILE_ADDONS_STORE).as_deref(),
        Some("Open the Addon Store")
    );
    assert_eq!(game_files(&env), before);
}

#[gpui_kit::test]
fn a_stored_graphics_none_with_addons_set_opens_and_saves_addons_none(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let vpk = support::vpk_bytes(&[("cfg/addon.cfg", b"echo addon\n")]);
    support::seed_library_addon(&s.services, &env, PYRO, "9.100.1", &vpk, false);
    let draft = ProfileDraft::new("Forbidden")
        .with_graphics(SlotMode::None)
        .with_addons(SlotMode::Set((vec![DraftItem::Pick(addon(PYRO))], true)));
    let p = s.services.library.create_profile(&draft).unwrap();
    let page = page(cx, &s);
    let before = game_files(&env);
    let e = edit(cx, &s, &page, p.id());
    assert_eq!(
        cx.update(|cx| e.read(cx).draft().mode(SlotKind::Addons)),
        Mode::None
    );
    assert_eq!(
        support::checked_of(cx, s.win, ids::profile_slot_mode("addons", "set")),
        Some(false)
    );
    support::click_id(cx, s.win, ids::PROFILE_EDITOR_SAVE);
    support::wait_real(cx, "the list", |cx| {
        cx.update(|cx| page.read(cx).editor().is_none())
    });
    let saved = s.services.library.profile(p.id()).unwrap();
    assert!(saved.graphics().as_set().is_none());
    assert!(saved.addons().as_set().is_none(), "add-ons left Set");
    drop(e);
    assert_eq!(game_files(&env), before);
}

#[gpui_kit::test]
fn an_edit_while_saving_is_refused_and_the_save_holds_the_draft(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let seeded = seed(&s, &env);
    let vpk = support::vpk_bytes(&[("cfg/addon.cfg", b"echo addon\n")]);
    for name in [PYRO, FLAT] {
        support::seed_library_addon(&s.services, &env, name, "9.100.1", &vpk, false);
    }
    let draft = ProfileDraft::new("Busy")
        .with_hud(SlotMode::Set(DraftItem::Pick(seeded.rayshud.id().clone())))
        .with_sounds(SlotMode::Set((
            Some(DraftItem::Pick(seeded.ding.id().clone())),
            None,
        )))
        .with_crosshairs(SlotMode::Set(Vec::new()))
        .with_graphics(SlotMode::Set(low_fps60()))
        .with_addons(SlotMode::Set((vec![DraftItem::Pick(addon(PYRO))], true)));
    let p = s.services.library.create_profile(&draft).unwrap();
    // Tall enough that every card is on screen, so each click lands without scrolling.
    cx.simulate_window_resize(s.win, size(px(1280.), px(4000.)));
    let page = page(cx, &s);
    let before = game_files(&env);
    let e = edit(cx, &s, &page, p.id());
    type_name(cx, &s, &e, "Busy two");
    let budhud = seeded.budhud.folder_name().to_owned();
    support::wait_real(cx, "the bodies", |cx| {
        support::present(cx, s.win, ids::profile_pick_hud(&budhud))
            && support::present(cx, s.win, ids::profile_addon(FLAT))
    });
    let dot = CrosshairRef::Builtin(s.services.builtins.all()[0].name().to_owned());
    let picker = cx.update(|cx| e.read(cx).picker().clone());
    cx.update(|cx| picker.update(cx, |p, cx| p.select(Some(dot), cx)));
    support::settle(cx, s.win);
    let gfx = tf2tools_app::ids::IdScope::PROFILE_GRAPHICS;
    let xh = tf2tools_app::ids::IdScope::PROFILE_CROSSHAIRS;
    let bell = SoundOption {
        id: Some(seeded.bell.id().clone()),
        label: "Bell".into(),
    };
    let kill = cx.update(|cx| e.read(cx).sound_select(SoundKind::Killsound).clone());
    // Save starts, the busy editor draws, then every kind of slot edit is tried before the
    // save's answer can land.
    let shown_disabled = cx
        .update_window(s.win, |_, w, cx| {
            e.update(cx, |e, cx| e.save(w, cx));
            w.render_frame(cx);
            let disabled = [
                ids::profile_slot_mode("hud", "none"),
                ids::profile_addon(FLAT),
                ids::PROFILE_ADDONS_ONLY.into(),
                gfx.id(ids::graphics_preset(Preset::High)),
                ids::PROFILE_XH_EVERYWHERE.into(),
            ]
            .map(|id| (id.clone(), support::button_disabled(w, id)));
            w.click(ids::profile_pick_hud(&budhud), cx);
            w.click(ids::profile_slot_mode("hud", "none"), cx);
            w.click(ids::profile_addon(FLAT), cx);
            w.click(gfx.id(ids::graphics_preset(Preset::High)), cx);
            w.click(
                xh.id(ids::loadout_cell(TfClass::Scout, WeaponSlot::Primary)),
                cx,
            );
            kill.update(cx, |_, cx| {
                cx.emit(SelectEvent::<Vec<SoundOption>>::Confirm(Some(bell)))
            });
            e.update(cx, |e, cx| {
                assert!(!e.set_mode(SlotKind::Graphics, Mode::None, cx));
                e.board_event(BoardEvent::Assign(BoardTarget::Everything), w, cx);
            });
            disabled
        })
        .unwrap();
    for (id, disabled) in shown_disabled {
        assert!(disabled, "{id:?} is drawn disabled while saving");
    }
    let draft = cx.update(|cx| e.read(cx).draft().clone());
    assert_eq!(
        draft.hud.as_ref().map(|h| h.folder.as_str()),
        Some(seeded.rayshud.folder_name())
    );
    assert_eq!(draft.mode(SlotKind::Hud), Mode::Set);
    assert_eq!(draft.mode(SlotKind::Graphics), Mode::Set);
    assert_eq!(draft.graphics.preset(), Preset::Low);
    assert_eq!(draft.addons.len(), 1);
    assert!(draft.crosshairs.is_empty());
    assert!(draft.kill.is_none());
    support::wait_real(cx, "the list", |cx| {
        cx.update(|cx| page.read(cx).editor().is_none())
    });
    let saved = s.services.library.profile(p.id()).unwrap();
    assert_eq!(saved.name(), "Busy two");
    assert_eq!(
        saved.hud().as_set().map(|h| h.folder_name()),
        Some(seeded.rayshud.folder_name())
    );
    assert_eq!(saved.addons().as_set().map(|a| a.on.len()), Some(1));
    assert_eq!(game_files(&env), before);
}

#[gpui_kit::test]
fn a_kept_addon_missing_from_the_library_still_shows_and_saves(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, found(&env));
    let vpk = support::vpk_bytes(&[("cfg/addon.cfg", b"echo addon\n")]);
    for name in [PYRO, FLAT] {
        support::seed_library_addon(&s.services, &env, name, "9.100.1", &vpk, false);
    }
    let draft = ProfileDraft::new("Kept")
        .with_graphics(SlotMode::Set(low_fps60()))
        .with_addons(SlotMode::Set((
            vec![DraftItem::keep_addon(addon(NULL))],
            false,
        )));
    let kept = s.services.library.create_profile(&draft).unwrap();
    cx.simulate_window_resize(s.win, size(px(1280.), px(4000.)));
    let page = page(cx, &s);
    let before = game_files(&env);
    let e = edit(cx, &s, &page, kept.id());
    support::wait_real(cx, "the kept switch", |cx| {
        support::present(cx, s.win, ids::profile_addon(NULL))
    });
    assert_eq!(
        support::label_of(cx, s.win, ids::profile_addon(NULL)).as_deref(),
        Some("Null-Canceling Movement")
    );
    assert_eq!(
        support::checked_of(cx, s.win, ids::profile_addon(NULL)),
        Some(true)
    );
    assert!(
        !cx.update(|cx| e.read(cx).is_dirty()),
        "a kept add-on is not an edit"
    );
    support::click_id(cx, s.win, ids::profile_addon(FLAT));
    support::click_id(cx, s.win, ids::PROFILE_EDITOR_SAVE);
    support::wait_real(cx, "the list", |cx| {
        cx.update(|cx| page.read(cx).editor().is_none())
    });
    let saved = s.services.library.profile(kept.id()).unwrap();
    let addons = saved.addons().as_set().expect("Set");
    assert_eq!(addons.on, [addon(FLAT), addon(NULL)]);
    assert_eq!(game_files(&env), before);
}
