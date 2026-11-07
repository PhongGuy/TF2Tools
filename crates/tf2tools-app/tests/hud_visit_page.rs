//! A library HUD from the store links to its own page; a HUD with no known page does not.

mod support;

use gpui_kit::TestAppContext;
use tf2tools_app::ids;
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_core::{GbId, HudEntry, Provenance, Tf2Install};

const REPO: &str = "https://github.com/raysfire/rayshud";
const GB_PAGE: &str = "https://gamebanana.com/mods/7";

fn shell(cx: &mut TestAppContext, env: &support::Env) -> support::ShellWindow {
    support::shell_window(
        cx,
        env,
        Tf2State::Found(Tf2Install::from_path(&env.game).unwrap()),
    )
}

fn import(
    s: &support::ShellWindow,
    env: &support::Env,
    root: &str,
    provenance: Provenance,
) -> HudEntry {
    support::hud_copies::import_stock_hud(&s.services, env.tmp.path(), root, &[], provenance)
}

fn gamebanana(id: u64) -> Provenance {
    Provenance::GameBanana {
        gb_id: GbId::new(id),
        author: "vex".into(),
    }
}

fn revisit_huds(cx: &mut TestAppContext, s: &support::ShellWindow) {
    support::click_id(cx, s.win, ids::nav(Page::Sounds));
    support::click_id(cx, s.win, ids::nav(Page::Huds));
}

// The store lists rayshud with its repository, GameBanana HUD 7 with its page and HUD 8
// without one.
fn seed_store(env: &support::Env) {
    support::seed_curated(env, "rayshud", "rayshud", "raysfire", Some(REPO), "{}");
    support::seed_gb_huds(env, &[(7, "vexhud"), (8, "nopage")]);
    rusqlite::Connection::open(env.data.db_path())
        .unwrap()
        .execute(
            "UPDATE store_huds_gb SET profile_url = ?1 WHERE gb_id = 7",
            [GB_PAGE],
        )
        .unwrap();
}

fn rayshud() -> Provenance {
    Provenance::Curated {
        hud_id: "rayshud".into(),
    }
}

#[gpui_kit::test]
fn only_huds_with_a_known_page_offer_visit_page(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    seed_store(&env);
    let curated = import(&s, &env, "rayshud", rayshud());
    // A curated HUD the store no longer lists has no known repository.
    let unlisted = import(
        &s,
        &env,
        "gonehud",
        Provenance::Curated {
            hud_id: "gonehud".into(),
        },
    );
    let local = import(&s, &env, "myhud", Provenance::Local);
    let copy = s.services.library.copy_hud(curated.id()).unwrap();
    revisit_huds(cx, &s);
    support::wait_real(cx, "the library", |cx| {
        support::present(cx, s.win, ids::hud_card(local.folder_name()))
    });
    assert_eq!(
        support::label_of(cx, s.win, ids::hud_visit(curated.folder_name())).as_deref(),
        Some("Visit page")
    );
    for hud in [&unlisted, &local, &copy] {
        assert!(support::present(
            cx,
            s.win,
            ids::hud_card(hud.folder_name())
        ));
        assert!(
            !support::present(cx, s.win, ids::hud_visit(hud.folder_name())),
            "{} has no known page",
            hud.name()
        );
    }
}

#[gpui_kit::test]
fn visit_page_opens_the_huds_own_page(cx: &mut TestAppContext) {
    let env = support::env();
    let s = shell(cx, &env);
    seed_store(&env);
    let curated = import(&s, &env, "rayshud", rayshud());
    let gb = import(&s, &env, "vexhud", gamebanana(7));
    // A GameBanana HUD whose row gives no address still has a page under its id.
    let gb_bare = import(&s, &env, "nopage", gamebanana(8));
    revisit_huds(cx, &s);
    support::wait_real(cx, "the library", |cx| {
        support::present(cx, s.win, ids::hud_visit(gb_bare.folder_name()))
    });
    // Every button stays inside its card, so none is clipped out of reach.
    for hud in [&curated, &gb, &gb_bare] {
        let card = support::bounds_of(cx, s.win, ids::hud_card(hud.folder_name())).unwrap();
        for id in [
            ids::hud_visit(hud.folder_name()),
            ids::hud_copy(hud.folder_name()),
        ] {
            let button = support::bounds_of(cx, s.win, id).unwrap();
            assert!(button.left() >= card.left() && button.right() <= card.right());
        }
    }
    support::click_id(cx, s.win, ids::hud_visit(curated.folder_name()));
    assert_eq!(cx.opened_url().as_deref(), Some(REPO));
    support::click_id(cx, s.win, ids::hud_visit(gb.folder_name()));
    assert_eq!(cx.opened_url().as_deref(), Some(GB_PAGE));
    support::click_id(cx, s.win, ids::hud_visit(gb_bare.folder_name()));
    assert_eq!(
        cx.opened_url().as_deref(),
        Some("https://gamebanana.com/mods/8")
    );
}
