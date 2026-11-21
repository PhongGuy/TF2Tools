//! The draft editor's slots: rail, modes, and each Set body. Editing never writes the game.

use crate::support;

use gpui_kit::component::select::SelectEvent;
use gpui_kit::{AppContext as _, TestAppContext, px, size};
use support::graphics::found;
use support::profiles::{edit, game_files, page, seed};
use tf2tools_app::audio::player::{NowPlaying, sound_player};
use tf2tools_app::ids;
use tf2tools_app::pages::profiles::draft_editor::SoundOption;
use tf2tools_app::pages::profiles::editor_model::{Mode, SlotKind};
use tf2tools_app::services::OpError;
use tf2tools_core::{DraftItem, ProfileDraft, SlotMode, SoundKind};

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
