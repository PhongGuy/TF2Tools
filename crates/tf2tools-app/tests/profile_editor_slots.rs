//! The draft editor's slots: rail, modes, and each Set body. Editing never writes the game.

use crate::support;

use gpui_kit::{TestAppContext, px, size};
use support::graphics::found;
use support::profiles::{edit, game_files, page, seed};
use tf2tools_app::ids;
use tf2tools_app::pages::profiles::editor_model::{Mode, SlotKind};
use tf2tools_core::{DraftItem, ProfileDraft, SlotMode};

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
