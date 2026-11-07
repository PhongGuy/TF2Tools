//! The Sounds page at small windows: the page scrolls as a whole over a grid of at least one card
//! row, slot text keeps clear of Uninstall, and the last card row scrolls clear of the player.

use crate::support;

use gpui_kit::{
    AppContext as _, Bounds, ElementId, Entity, Pixels, ScrollHandle, TestAppContext, point, px,
    size,
};
use support::scroll::{
    assert_bar_beside, assert_bar_on_right_edge, assert_track_scrolls,
    assert_track_scrolls_virtual, assert_track_stays, bounds,
};
use tf2tools_app::ids;
use tf2tools_app::pages::sounds::SoundsPage;
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_app::shell::toast::{ToastKind, toast};
use tf2tools_core::{Provenance, SoundEntry, SoundKind, Tf2Install};

fn tf2(env: &support::Env) -> Tf2Install {
    Tf2Install::from_path(&env.game).unwrap()
}

fn resize(cx: &mut TestAppContext, s: &support::ShellWindow, w: f32, h: f32) {
    cx.simulate_window_resize(s.win, size(px(w), px(h)));
    support::settle(cx, s.win);
}

/// Opens the Sounds page and waits for its load.
fn open_sounds(cx: &mut TestAppContext, s: &support::ShellWindow) -> Entity<SoundsPage> {
    support::click_id(cx, s.win, ids::nav(Page::Sounds));
    let page = cx.update(|cx| s.shell.read(cx).sounds_page()).unwrap();
    support::wait_real(cx, "Sounds to load", |cx| {
        cx.update(|cx| {
            let page = page.read(cx);
            page.is_loaded() && page.player().read(cx).is_ready()
        })
    });
    support::settle(cx, s.win);
    page
}

/// A TF2-playable WAV: 16-bit stereo PCM at 44.1 kHz, 441 frames of silence.
fn wav() -> Vec<u8> {
    let data_len: u32 = 441 * 4;
    let mut out = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&44_100u32.to_le_bytes());
    out.extend_from_slice(&(44_100u32 * 4).to_le_bytes());
    out.extend_from_slice(&4u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    out.resize(out.len() + data_len as usize, 0);
    out
}

/// Imports a WAV named `name` into the library.
fn seed(s: &support::ShellWindow, env: &support::Env, name: &str) -> SoundEntry {
    let path = env.tmp.path().join(format!("{name}.wav"));
    std::fs::write(&path, wav()).unwrap();
    s.services
        .library
        .import_sound(&path, name, Provenance::Local)
        .unwrap()
}

/// Seeds `n` sounds named "Sound 00" and on; returns their card ids in name order.
fn seed_many(s: &support::ShellWindow, env: &support::Env, n: usize) -> Vec<ElementId> {
    (0..n)
        .map(|ix| ids::sound_card(seed(s, env, &format!("Sound {ix:02}")).folder_name()))
        .collect()
}

/// The issue's sample: a hand-placed hitsound and "Classic Ding" installed as the killsound.
fn seed_both_slots(s: &support::ShellWindow, env: &support::Env) {
    let dir = env.game.join("tf/custom/tf2tools-sounds/sound/ui");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("hitsound.wav"), wav()).unwrap();
    let ding = seed(s, env, "Classic Ding");
    s.services
        .library
        .install_sound(ding.id(), SoundKind::Killsound, &tf2(env))
        .unwrap();
}

fn page_scroll(cx: &mut TestAppContext, page: &Entity<SoundsPage>) -> ScrollHandle {
    cx.update(|cx| page.read(cx).page_scroll())
}

fn grid_scroll(cx: &mut TestAppContext, page: &Entity<SoundsPage>) -> ScrollHandle {
    cx.update(|cx| page.read(cx).grid_scroll())
}

/// The bounds of every card in `cards` the grid draws now.
fn drawn(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    cards: &[ElementId],
) -> Vec<Bounds<Pixels>> {
    cards
        .iter()
        .filter_map(|id| support::bounds_of(cx, s.win, id.clone()))
        .collect()
}

fn to_end(cx: &mut TestAppContext, s: &support::ShellWindow, scroll: &ScrollHandle) {
    scroll.set_offset(point(px(0.), -scroll.max_offset().y));
    support::settle(cx, s.win);
}

// A slot's name gives way before Uninstall, so the two never touch at 900x600.
#[gpui_kit::test]
fn at_900x600_slot_names_keep_clear_of_uninstall(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Found(tf2(&env)));
    seed_both_slots(&s, &env);
    resize(cx, &s, 900., 600.);
    let _page = open_sounds(cx, &s);
    for kind in [SoundKind::Hitsound, SoundKind::Killsound] {
        let (slot, name, uninstall) = (
            bounds(cx, s.win, ids::sound_slot(kind)),
            bounds(cx, s.win, ids::sound_slot_name(kind)),
            bounds(cx, s.win, ids::sound_slot_uninstall(kind)),
        );
        assert!(
            name.right() + px(8.) <= uninstall.left(),
            "the {kind:?} name {name:?} touches Uninstall {uninstall:?}"
        );
        assert!(
            uninstall.right() <= slot.right() && name.left() >= slot.left(),
            "the {kind:?} slot {slot:?} does not hold {name:?} and {uninstall:?}"
        );
    }
}

// An installed slot and an empty one are one height, as v5's two-column grid draws them.
#[gpui_kit::test]
fn the_slot_cards_share_one_height(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Found(tf2(&env)));
    let ding = seed(&s, &env, "Classic Ding");
    s.services
        .library
        .install_sound(ding.id(), SoundKind::Hitsound, &tf2(&env))
        .unwrap();
    for (w, h) in [(900., 600.), (1280., 800.)] {
        resize(cx, &s, w, h);
        let _page = open_sounds(cx, &s);
        let (hit, kill) = (
            bounds(cx, s.win, ids::sound_slot(SoundKind::Hitsound)),
            bounds(cx, s.win, ids::sound_slot(SoundKind::Killsound)),
        );
        assert_eq!(hit.size.height, kill.size.height, "at {w}x{h}");
    }
}

// Scrolls the grid to its end under a fresh toast: the last row ends inside the grid and above
// both the floating player and the toast.
fn assert_last_row_clears_the_player(
    cx: &mut TestAppContext,
    s: &support::ShellWindow,
    page: &Entity<SoundsPage>,
    cards: &[ElementId],
) {
    let grid_scroll = grid_scroll(cx, page);
    to_end(cx, s, &grid_scroll);
    cx.update_window(s.win, |_, window, cx| {
        toast(window, cx, ToastKind::Success, "Hitsound installed", "")
    })
    .unwrap();
    support::settle(cx, s.win);
    let (grid, player, note) = (
        bounds(cx, s.win, ids::SOUNDS_GRID),
        bounds(cx, s.win, ids::SOUND_PLAYER),
        bounds(cx, s.win, ids::TOAST),
    );
    let last = drawn(cx, s, cards)
        .into_iter()
        .map(|b| b.bottom())
        .fold(px(0.), Pixels::max);
    assert!(
        last <= grid.bottom() + px(1.) && last <= player.top().min(note.top()),
        "the last row ends at {last:?}: past the grid {grid:?}, the player {player:?} or the \
         toast {note:?}"
    );
}

// At 900x600 the page scrolls as a whole to a grid at least one card tall, and at the page's end
// the grid's last row scrolls clear of the floating player and its toast.
#[gpui_kit::test]
fn at_900x600_the_page_scrolls_to_a_card_row_clear_of_the_player(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Found(tf2(&env)));
    seed_both_slots(&s, &env);
    let cards = seed_many(&s, &env, 12);
    resize(cx, &s, 900., 600.);
    let page = open_sounds(cx, &s);
    let area = ids::page(Page::Sounds);
    assert_bar_on_right_edge(cx, s.win, area.clone(), ids::page_scrollbar(Page::Sounds));
    assert_track_scrolls(cx, s.win, area.clone(), ids::SOUNDS_LIBRARY);
    let bar = ids::grid_scrollbar(ids::SOUNDS_GRID);
    assert_bar_beside(cx, s.win, ids::SOUNDS_GRID, bar.clone());
    let scroll = page_scroll(cx, &page);
    to_end(cx, &s, &scroll);
    let card = drawn(cx, &s, &cards)[0];
    let (grid, area) = (bounds(cx, s.win, ids::SOUNDS_GRID), bounds(cx, s.win, area));
    assert!(
        grid.size.height >= card.size.height && grid.bottom() <= area.bottom() + px(1.),
        "the grid {grid:?} shows no full {card:?} row in the page {area:?}"
    );
    let first = cards
        .iter()
        .find(|id| support::present(cx, s.win, (*id).clone()))
        .unwrap()
        .clone();
    assert_track_scrolls_virtual(cx, s.win, bar, first);
    assert_last_row_clears_the_player(cx, &s, &page, &cards);
}

// At a common size the page fits and the grid runs to the page's 48 px margin, as on HUDs; only
// the grid's bar shows, and its last row still scrolls clear of the player.
#[gpui_kit::test]
fn at_1280x800_only_the_sounds_grid_scrolls(cx: &mut TestAppContext) {
    let env = support::env();
    let s = support::shell_window(cx, &env, Tf2State::Found(tf2(&env)));
    let cards = seed_many(&s, &env, 24);
    resize(cx, &s, 1280., 800.);
    let page = open_sounds(cx, &s);
    let bar = ids::grid_scrollbar(ids::SOUNDS_GRID);
    assert_bar_beside(cx, s.win, ids::SOUNDS_GRID, bar.clone());
    assert_track_stays(cx, s.win, ids::page(Page::Sounds), ids::SOUNDS_LIBRARY);
    let grid = bounds(cx, s.win, ids::SOUNDS_GRID);
    assert!(
        grid.bottom() >= px(800. - 48. - 1.),
        "the grid {grid:?} stops short of the window's 48 px margin"
    );
    assert_track_scrolls_virtual(cx, s.win, bar, cards[0].clone());
    assert_last_row_clears_the_player(cx, &s, &page, &cards);
}
