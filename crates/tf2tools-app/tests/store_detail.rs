mod support;

use std::time::Duration;

use gpui_kit::component::WindowExt as _;
use gpui_kit::px;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};
use support::scroll::{assert_bar_beside, assert_clear_of_bar, assert_track_scrolls, bounds};
use tf2tools_app::ids;
use tf2tools_app::pages::hud_store::bodies::{curated_body, gamebanana_body, social_icon};
use tf2tools_app::store_detail::detail_width;
use tf2tools_app::store_detail::{
    DetailItem, FilesState, StoreDetail, is_unsupported, open_detail,
};
use tf2tools_app::store_download::StoreDownloads;
use tf2tools_core::{GbId, StoreItemId, StoreKind};

const GB: StoreKind = StoreKind::GameBananaHuds;

fn gb(id: u64) -> StoreItemId {
    StoreItemId::GameBanana(GbId::new(id))
}

fn item(kind: StoreKind, id: StoreItemId, name: &str, in_library: bool) -> DetailItem {
    DetailItem {
        kind,
        id,
        name: name.into(),
        in_library,
    }
}

fn detail(
    cx: &mut TestAppContext,
    win: AnyWindowHandle,
    item: DetailItem,
    curated: bool,
) -> Entity<StoreDetail> {
    let body = if curated {
        curated_body()
    } else {
        gamebanana_body()
    };
    let d = cx
        .update_window(win, |_, window, cx| open_detail(item, body, window, cx))
        .unwrap();
    assert!(support::present(cx, win, ids::STORE_DETAIL));
    d
}

fn downloads(cx: &mut TestAppContext) -> Entity<StoreDownloads> {
    cx.update(|cx| StoreDownloads::global(cx)).unwrap()
}

#[test]
fn rar_files_are_unsupported() {
    assert!(is_unsupported("hud.rar"));
    assert!(is_unsupported("HUD.RAR"));
    assert!(!is_unsupported("hud.zip"));
    assert!(!is_unsupported("rar.7z"));
}

#[gpui_kit::test]
fn a_gamebanana_item_lists_its_files_and_downloads_one(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let files = [
        ("flawhud.zip", support::hud_zip_bytes("flawhud")),
        ("flawhud.rar", b"rar".to_vec()),
    ];
    server.mount_gb_files(7, &files, Duration::ZERO);
    let (win, _) = support::store_window(
        cx,
        &env,
        &server,
        |env| support::seed_gb_huds(env, &[(7, "FlawHUD")]),
        |_, _| support::Blank,
    );
    let d = detail(cx, win, item(GB, gb(7), "FlawHUD", false), false);
    support::wait_real(cx, "the files", |cx| {
        support::present(cx, win, ids::store_file(1))
    });
    assert_eq!(
        cx.update(|cx| d.read(cx).carousel().read(cx).len()),
        2,
        "the item's gallery"
    );
    assert!(
        support::present(cx, win, ids::store_file_note(1)),
        "the RAR is marked"
    );
    assert!(!support::present(cx, win, ids::store_file_note(0)));
    support::click_id(cx, win, ids::store_file(1));
    let dl = downloads(cx);
    assert!(
        !cx.update(|cx| dl.read(cx).is_busy()),
        "an unsupported file never starts"
    );
    support::click_id(cx, win, ids::store_file(0));
    support::wait_real(cx, "the download", |cx| {
        cx.update(|cx| !dl.read(cx).is_busy())
    });
    support::wait_real(cx, "the badge", |cx| {
        support::present(cx, win, ids::STORE_DETAIL_BADGE)
    });
    assert_eq!(
        support::label_of(cx, win, ids::STORE_DETAIL_BADGE).as_deref(),
        Some("In Library")
    );
    assert!(cx.update(|cx| d.read(cx).item().in_library));
}

// The toast lands outside the dialog's box at this size, so a click that reached the dimming
// behind it would close the dialog.
#[gpui_kit::test]
fn a_click_on_a_failure_toast_leaves_the_detail_open(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (win, _) = support::store_window(
        cx,
        &env,
        &server,
        |env| support::seed_gb_huds(env, &[(9, "Gone")]),
        |_, _| support::Blank,
    );
    let _d = detail(cx, win, item(GB, gb(9), "Gone", false), false);
    support::wait_real(cx, "the toast", |cx| support::present(cx, win, ids::TOAST));
    support::click_id(cx, win, ids::TOAST);
    assert!(support::present(cx, win, ids::STORE_DETAIL));
}

#[gpui_kit::test]
fn a_failed_listing_shows_inline_toasts_and_retries(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (win, _) = support::store_window(
        cx,
        &env,
        &server,
        |env| support::seed_gb_huds(env, &[(9, "Gone")]),
        |_, _| support::Blank,
    );
    let _d = detail(cx, win, item(GB, gb(9), "Gone", false), false);
    support::wait_real(cx, "the inline error", |cx| {
        support::present(cx, win, ids::STORE_FILES_ERROR)
    });
    support::wait_real(cx, "the toast", |cx| support::present(cx, win, ids::TOAST));
    let toast = support::label_of(cx, win, ids::TOAST).unwrap();
    assert!(toast.starts_with("Failed to load files. "), "{toast}");
    assert!(
        !toast.to_lowercase().contains("download"),
        "opening a detail downloads nothing: {toast}"
    );
    let inline = support::label_of(cx, win, ids::STORE_FILES_ERROR).unwrap();
    support::assert_plain(&inline);
    assert!(inline.contains("GameBanana"), "{inline}");
    assert_eq!(toast, format!("Failed to load files. {inline}"));
    server.mount_gb_files(
        9,
        &[("gone.zip", support::hud_zip_bytes("gone"))],
        Duration::ZERO,
    );
    support::click_id(cx, win, ids::STORE_FILES_RETRY);
    support::wait_real(cx, "the files", |cx| {
        support::present(cx, win, ids::store_file(0))
    });
    assert!(!support::present(cx, win, ids::STORE_FILES_ERROR));
}

#[gpui_kit::test]
fn a_running_download_disables_other_file_buttons(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    server.mount_gb_files(
        7,
        &[("a.zip", support::hud_zip_bytes("ahud"))],
        Duration::ZERO,
    );
    server.mount_gb_files(
        8,
        &[("b.zip", support::hud_zip_bytes("bhud"))],
        Duration::from_millis(800),
    );
    let (win, _) = support::store_window(
        cx,
        &env,
        &server,
        |env| support::seed_gb_huds(env, &[(7, "A"), (8, "B")]),
        |_, _| support::Blank,
    );
    let other = detail(cx, win, item(GB, gb(8), "B", false), false);
    support::wait_real(cx, "B's files", |cx| {
        support::present(cx, win, ids::store_file(0))
    });
    assert!(cx.update(|cx| other.update(cx, |d, cx| d.download(0, cx))));
    cx.update_window(win, |_, window, cx| window.close_dialog(cx))
        .unwrap();
    let d = detail(cx, win, item(GB, gb(7), "A", false), false);
    support::wait_real(cx, "A's files", |cx| {
        cx.update(|cx| matches!(d.read(cx).files(), FilesState::Ready(_)))
    });
    support::click_id(cx, win, ids::store_file(0));
    let dl = downloads(cx);
    assert_eq!(
        cx.update(|cx| dl.read(cx).active().map(|t| t.item.clone())),
        Some(gb(8)),
        "A did not start"
    );
    assert!(!cx.update(|cx| d.update(cx, |d, cx| d.download(0, cx))));
    support::wait_real(cx, "B to finish", |cx| {
        cx.update(|cx| !dl.read(cx).is_busy())
    });
    assert!(cx.update(|cx| d.update(cx, |d, cx| d.download(0, cx))));
}

#[gpui_kit::test]
fn a_curated_item_shows_its_data_and_one_archive(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let repo = server.mount_curated("rayshud", support::hud_zip_bytes("rayshud"), Duration::ZERO);
    let json = r#"{"flags":["minmode"],"traits":{"os":["linux"]},"resources":["banner","menu"]}"#;
    let (win, _) = support::store_window(
        cx,
        &env,
        &server,
        move |env| support::seed_curated(env, "rayshud", "rayshud", "raysfire", Some(&repo), json),
        |_, _| support::Blank,
    );
    let d = detail(
        cx,
        win,
        item(
            StoreKind::CuratedHuds,
            StoreItemId::Curated("rayshud".into()),
            "rayshud",
            true,
        ),
        true,
    );
    support::wait_real(cx, "the archive", |cx| {
        support::present(cx, win, ids::store_file(0))
    });
    assert!(!support::present(cx, win, ids::store_file(1)));
    assert_eq!(
        support::label_of(cx, win, ids::STORE_DETAIL_BADGE).as_deref(),
        Some("Downloaded")
    );
    assert_eq!(
        cx.update(|cx| d.read(cx).carousel().read(cx).len()),
        2,
        "one screenshot per resource"
    );
}

#[test]
fn sound_details_are_v5_s_lg_and_the_rest_2xl() {
    assert_eq!(detail_width(StoreKind::Sounds), px(512.));
    for kind in [
        StoreKind::GameBananaHuds,
        StoreKind::CuratedHuds,
        StoreKind::Crosshairs,
        StoreKind::Addons,
    ] {
        assert_eq!(detail_width(kind), px(672.), "{kind:?}");
    }
}

#[gpui_kit::test]
fn the_detail_is_2xl_centred_with_the_badge_after_the_name(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (win, _) = support::store_window(
        cx,
        &env,
        &server,
        |env| support::seed_gb_huds(env, &[(7, "FlawHUD")]),
        |_, _| support::Blank,
    );
    let _d = detail(cx, win, item(GB, gb(7), "FlawHUD", true), false);
    let b = support::dialog_box(cx, win);
    let view = support::viewport(cx, win);
    assert_eq!(b.size.width, px(672.));
    let off = b.origin.y + b.size.height / 2. - view.height / 2.;
    assert!(off.abs() <= px(1.), "box {b:?} in {view:?}");
    let name = support::bounds_of(cx, win, ids::STORE_DETAIL_NAME).unwrap();
    let badge = support::bounds_of(cx, win, ids::STORE_DETAIL_BADGE).unwrap();
    assert_eq!(badge.origin.x - (name.origin.x + name.size.width), px(8.));
    let mid = |y: gpui_kit::Pixels, h: gpui_kit::Pixels| y + h / 2.;
    assert!(
        (mid(badge.origin.y, badge.size.height) - mid(name.origin.y, name.size.height)).abs()
            <= px(1.)
    );
}

// v5's curated detail: flags as secondary badges, tags as outline ones, both 21 px.
#[gpui_kit::test]
fn a_curated_detail_draws_flags_and_tags_as_21_px_badges(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let repo = server.mount_curated("rayshud", support::hud_zip_bytes("rayshud"), Duration::ZERO);
    let json = r#"{"flags":["minmode"],"tags":["menus"],"traits":{"os":["linux"]},"resources":["banner"]}"#;
    let (win, _) = support::store_window(
        cx,
        &env,
        &server,
        move |env| support::seed_curated(env, "rayshud", "rayshud", "raysfire", Some(&repo), json),
        |_, _| support::Blank,
    );
    let _d = detail(
        cx,
        win,
        item(
            StoreKind::CuratedHuds,
            StoreItemId::Curated("rayshud".into()),
            "rayshud",
            false,
        ),
        true,
    );
    support::wait_real(cx, "the flags", |cx| {
        support::present(cx, win, ids::store_detail_tag("flags", 0))
    });
    for (id, text) in [
        (ids::store_detail_tag("flags", 0), "minmode"),
        (ids::store_detail_tag("tags", 0), "menus"),
    ] {
        assert_eq!(
            support::label_of(cx, win, id.clone()).as_deref(),
            Some(text)
        );
        let badge = support::bounds_of(cx, win, id).expect("a badge");
        assert_eq!(badge.size.height, px(21.), "{text}");
    }
}

// `text` shaped semibold in the theme font at `size`, as the window lays it out.
fn semibold_width(
    cx: &mut TestAppContext,
    win: AnyWindowHandle,
    text: &str,
    size: gpui_kit::Pixels,
) -> gpui_kit::Pixels {
    use gpui_kit::component::ActiveTheme as _;
    cx.update_window(win, |_, w, cx| {
        let font = gpui_kit::Font {
            weight: gpui_kit::FontWeight::SEMIBOLD,
            ..gpui_kit::font(cx.theme().font_family.clone())
        };
        let run = gpui_kit::TextRun {
            len: text.len(),
            font,
            ..Default::default()
        };
        w.text_system()
            .shape_line(text.to_owned().into(), size, &[run], None)
            .width
    })
    .unwrap()
}

// v5's detail name is `text-xl font-semibold`: 20 px text on a 28 px line.
#[gpui_kit::test]
fn the_detail_name_is_v5_s_text_xl(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (win, _) = support::store_window(
        cx,
        &env,
        &server,
        |env| support::seed_gb_huds(env, &[(7, "FlawHUD")]),
        |_, _| support::Blank,
    );
    let _d = detail(cx, win, item(GB, gb(7), "FlawHUD", false), false);
    let name = support::bounds_of(cx, win, ids::STORE_DETAIL_NAME).unwrap();
    assert_eq!(name.size.height, px(28.), "text-xl's line");
    let xl = semibold_width(cx, win, "FlawHUD", px(20.));
    assert!(
        (name.size.width - xl).abs() <= px(0.5),
        "the name is {:?} wide; at 20 px it is {xl:?}",
        name.size.width
    );
}

// v5's detail badge rows are `flex flex-wrap gap-1.5`: 6 px between badges.
#[gpui_kit::test]
fn detail_badges_sit_v5_s_6_px_apart(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let repo = server.mount_curated("rayshud", support::hud_zip_bytes("rayshud"), Duration::ZERO);
    let json =
        r#"{"flags":["minmode","widescreen"],"tags":["menus","minimal"],"resources":["banner"]}"#;
    let (win, _) = support::store_window(
        cx,
        &env,
        &server,
        move |env| support::seed_curated(env, "rayshud", "rayshud", "raysfire", Some(&repo), json),
        |_, _| support::Blank,
    );
    let _d = detail(
        cx,
        win,
        item(
            StoreKind::CuratedHuds,
            StoreItemId::Curated("rayshud".into()),
            "rayshud",
            false,
        ),
        true,
    );
    support::wait_real(cx, "the badges", |cx| {
        support::present(cx, win, ids::store_detail_tag("tags", 1))
    });
    for row in ["flags", "tags"] {
        let first = support::bounds_of(cx, win, ids::store_detail_tag(row, 0)).unwrap();
        let second = support::bounds_of(cx, win, ids::store_detail_tag(row, 1)).unwrap();
        assert_eq!(second.top(), first.top(), "{row}: one line");
        assert_eq!(second.left() - first.right(), px(6.), "{row}");
    }
}

// v5's detail header: the name block left, its outline link button at the top right beside it.
fn assert_link_beside_name(
    cx: &mut TestAppContext,
    win: AnyWindowHandle,
    d: &Entity<StoreDetail>,
    key: &str,
) {
    // The dialog re-centres whenever the file listing lands and changes its height, so wait for
    // the listing to finish, then take every box from one frame so a late landing cannot tear them.
    support::wait_real(cx, "the link button and the file listing", |cx| {
        support::present(cx, win, ids::store_detail_link(key))
            && cx.update(|cx| !matches!(d.read(cx).files(), FilesState::Loading))
    });
    support::settle(cx, win);
    let [detail, name, link, close] = cx
        .update_window(win, |_, w, _| {
            let dialog = w.within("dialog");
            [
                ids::STORE_DETAIL.into(),
                ids::STORE_DETAIL_NAME.into(),
                ids::store_detail_link(key),
                "close".into(),
            ]
            .map(|id| {
                dialog
                    .try_find(id)
                    .map(|e| e.bounds())
                    .expect("a header box")
            })
        })
        .unwrap();
    assert_eq!(
        link.top(),
        name.top(),
        "{key}: top-aligned with the name block"
    );
    assert!(
        link.left() > name.right(),
        "{key}: {link:?} right of {name:?}"
    );
    assert!(link.right() < detail.right(), "{key}: inside {detail:?}");
    assert_eq!(
        close.left() - link.right(),
        px(8.),
        "{key}: right-most, 8 px clear of the close X"
    );
    assert_eq!(link.size.height, px(32.), "{key}: v5's sm button");
}

#[gpui_kit::test]
fn the_repository_button_sits_beside_the_curated_name(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let repo = server.mount_curated("rayshud", support::hud_zip_bytes("rayshud"), Duration::ZERO);
    let (win, _) = support::store_window(
        cx,
        &env,
        &server,
        move |env| {
            support::seed_curated(
                env,
                "rayshud",
                "rayshud",
                "raysfire",
                Some(&repo),
                r#"{"resources":["banner"]}"#,
            )
        },
        |_, _| support::Blank,
    );
    let d = detail(
        cx,
        win,
        item(
            StoreKind::CuratedHuds,
            StoreItemId::Curated("rayshud".into()),
            "rayshud",
            false,
        ),
        true,
    );
    assert_link_beside_name(cx, win, &d, "repo");
}

#[gpui_kit::test]
fn the_gamebanana_button_sits_beside_the_name(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (win, _) = support::store_window(
        cx,
        &env,
        &server,
        |env| {
            support::seed_gb_huds(env, &[(7, "FlawHUD")]);
            rusqlite::Connection::open(env.data.db_path())
                .unwrap()
                .execute(
                    "UPDATE store_huds_gb SET profile_url = 'https://gamebanana.com/mods/7' WHERE gb_id = 7",
                    (),
                )
                .unwrap();
        },
        |_, _| support::Blank,
    );
    let d = detail(cx, win, item(GB, gb(7), "FlawHUD", false), false);
    assert_link_beside_name(cx, win, &d, "gamebanana");
}

// A GameBanana row without a profile page has nothing to open, so its header draws no button.
#[gpui_kit::test]
fn a_gamebanana_item_without_a_profile_url_has_no_header_button(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let (win, _) = support::store_window(
        cx,
        &env,
        &server,
        |env| support::seed_gb_huds(env, &[(7, "FlawHUD")]),
        |_, _| support::Blank,
    );
    let _d = detail(cx, win, item(GB, gb(7), "FlawHUD", false), false);
    support::wait_real(cx, "the stats row", |cx| {
        support::present(cx, win, ids::STORE_DETAIL_LIKES)
    });
    assert!(!support::present(
        cx,
        win,
        ids::store_detail_link("gamebanana")
    ));
}

// The socials keep their ghost row under `social:<key>` ids, so a social named "repo" and the
// header's Repository button are two elements.
#[gpui_kit::test]
fn curated_socials_are_keyed_apart_from_the_header_button(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let repo = server.mount_curated("rayshud", support::hud_zip_bytes("rayshud"), Duration::ZERO);
    let json =
        r#"{"social":{"discord":"https://example.invalid/d","repo":"https://example.invalid/r"}}"#;
    let (win, _) = support::store_window(
        cx,
        &env,
        &server,
        move |env| support::seed_curated(env, "rayshud", "rayshud", "raysfire", Some(&repo), json),
        |_, _| support::Blank,
    );
    let _d = detail(
        cx,
        win,
        item(
            StoreKind::CuratedHuds,
            StoreItemId::Curated("rayshud".into()),
            "rayshud",
            false,
        ),
        true,
    );
    support::wait_real(cx, "the socials", |cx| {
        support::present(cx, win, ids::store_detail_link("social:discord"))
    });
    let header = support::bounds_of(cx, win, ids::store_detail_link("repo")).unwrap();
    let social = support::bounds_of(cx, win, ids::store_detail_link("social:repo")).unwrap();
    assert!(
        social.top() > header.bottom(),
        "{social:?} below {header:?}"
    );
}

#[gpui_kit::test]
fn a_failed_row_read_toasts_and_still_lists_the_files(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    server.mount_gb_files(
        7,
        &[("flawhud.zip", support::hud_zip_bytes("flawhud"))],
        Duration::ZERO,
    );
    let (win, _) = support::store_window(
        cx,
        &env,
        &server,
        |env| {
            support::seed_gb_huds(env, &[(7, "FlawHUD")]);
            support::rename_table(env, "store_huds_gb", "store_huds_gb_hidden");
        },
        |_, _| support::Blank,
    );
    let d = detail(cx, win, item(GB, gb(7), "FlawHUD", false), false);
    support::wait_real(cx, "the files", |cx| {
        support::present(cx, win, ids::store_file(0))
    });
    support::wait_real(cx, "the toast", |cx| support::present(cx, win, ids::TOAST));
    let toasts = support::toast_labels(cx, win);
    assert_eq!(
        toasts,
        [
            "Failed to load details. TF2Tools couldn't read the store. Restart TF2Tools and try again."
        ]
    );
    support::assert_plain(&toasts[0]);
    assert!(cx.update(|cx| d.read(cx).store_item().is_none()));
    assert_eq!(cx.update(|cx| d.read(cx).carousel().read(cx).len()), 0);
}

#[test]
fn each_social_kind_gets_its_v5_icon() {
    for (key, want) in [
        ("discord", "MessageCircle"),
        ("support", "Heart"),
        ("twitter", "ExternalLink"),
        ("steam_group", "ExternalLink"),
    ] {
        assert_eq!(format!("{:?}", social_icon(key)), want, "{key}");
    }
}

// v5's social buttons lead with a size-3.5 icon, gap-1.5 (6 px) before the label. All boxes
// come from one frame inside the dialog, so a late file listing that re-centres it cannot tear them.
#[gpui_kit::test]
fn curated_socials_lead_with_a_14_px_icon(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let repo = server.mount_curated("rayshud", support::hud_zip_bytes("rayshud"), Duration::ZERO);
    let json = r#"{"social":{"discord":"https://example.invalid/d","support":"https://example.invalid/s","twitter":"https://example.invalid/t"}}"#;
    let (win, _) = support::store_window(
        cx,
        &env,
        &server,
        move |env| support::seed_curated(env, "rayshud", "rayshud", "raysfire", Some(&repo), json),
        |_, _| support::Blank,
    );
    let _d = detail(
        cx,
        win,
        item(
            StoreKind::CuratedHuds,
            StoreItemId::Curated("rayshud".into()),
            "rayshud",
            false,
        ),
        true,
    );
    support::wait_real(cx, "the socials", |cx| {
        support::present(cx, win, ids::store_detail_link("social:twitter"))
    });
    assert_eq!(
        support::label_of(cx, win, ids::store_detail_link("social:discord")).as_deref(),
        Some("Discord")
    );
    support::settle(cx, win);
    let boxes = cx
        .update_window(win, |_, w, _| {
            let dialog = w.within("dialog");
            ["discord", "support", "twitter"].map(|key| {
                let button = dialog
                    .try_find(ids::store_detail_link(&format!("social:{key}")))
                    .map(|e| e.bounds());
                let icon = dialog
                    .try_find(ids::store_detail_social_icon(key))
                    .map(|e| e.bounds());
                let label = dialog
                    .try_find(ids::store_detail_social_label(key))
                    .map(|e| e.bounds());
                (key, button, icon, label)
            })
        })
        .unwrap();
    for (key, button, icon, label) in boxes {
        let button = button.unwrap_or_else(|| panic!("{key}: the button"));
        let icon = icon.unwrap_or_else(|| panic!("{key}: the icon"));
        let label = label.unwrap_or_else(|| panic!("{key}: the label"));
        assert_eq!(
            (icon.size.width, icon.size.height),
            (px(14.), px(14.)),
            "{key}"
        );
        assert!(
            button.contains(&icon.center()),
            "{key}: {icon:?} in {button:?}"
        );
        assert!(
            icon.center().x < button.center().x,
            "{key}: the icon leads the label, {icon:?} in {button:?}"
        );
        assert_eq!(label.left() - icon.right(), px(6.), "{key}: v5's gap-1.5");
        // v5's sm button: 32 px high, 10 px either side once it holds an icon, label inside.
        assert_eq!(button.size.height, px(32.), "{key}: v5's h-8");
        assert_eq!(icon.left() - button.left(), px(10.), "{key}: v5's px-2.5");
        assert_eq!(
            button.right() - label.right(),
            px(10.),
            "{key}: v5's px-2.5"
        );
    }
}

// v5 caps a detail at 90% of the window and scrolls its body, so in a window shorter than
// the minimum the box fills that cap and the carousel keeps its 16:9 height.
#[gpui_kit::test]
fn a_short_window_scrolls_the_detail_instead_of_squashing_the_carousel(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let repo = server.mount_curated("rayshud", support::hud_zip_bytes("rayshud"), Duration::ZERO);
    let json = r#"{"flags":["minmode"],"traits":{"os":["linux"]},"resources":["banner","menu"]}"#;
    let (win, _) = support::store_window(
        cx,
        &env,
        &server,
        move |env| support::seed_curated(env, "rayshud", "rayshud", "raysfire", Some(&repo), json),
        |_, _| support::Blank,
    );
    cx.simulate_window_resize(win, gpui_kit::size(px(1280.), px(420.)));
    let rayshud = StoreItemId::Curated("rayshud".into());
    detail(
        cx,
        win,
        item(StoreKind::CuratedHuds, rayshud, "rayshud", true),
        true,
    );
    support::wait_real(cx, "the archive", |cx| {
        support::present(cx, win, ids::store_file(0))
    });
    let b = support::dialog_box(cx, win);
    assert!((b.size.height - px(378.)).abs() <= px(1.), "box {b:?}");
    let image = support::bounds_of(cx, win, ids::CAROUSEL_IMAGE).unwrap();
    let want = image.size.width * (9. / 16.);
    assert!(
        (image.size.height - want).abs() <= px(1.),
        "carousel {image:?}"
    );
}

// The header stays while the files, carousel and body scroll beside a bar in the box padding,
// below the close X and clear of the Download buttons.
#[gpui_kit::test]
fn a_short_window_shows_a_bar_beside_the_detail_body(cx: &mut TestAppContext) {
    let env = support::env();
    let server = support::StoreServer::start();
    let repo = server.mount_curated("rayshud", support::hud_zip_bytes("rayshud"), Duration::ZERO);
    let json = r#"{"flags":["minmode"],"traits":{"os":["linux"]},"resources":["banner","menu"]}"#;
    let (win, _) = support::store_window(
        cx,
        &env,
        &server,
        move |env| support::seed_curated(env, "rayshud", "rayshud", "raysfire", Some(&repo), json),
        |_, _| support::Blank,
    );
    cx.simulate_window_resize(win, gpui_kit::size(px(1280.), px(420.)));
    let rayshud = StoreItemId::Curated("rayshud".into());
    detail(
        cx,
        win,
        item(StoreKind::CuratedHuds, rayshud, "rayshud", true),
        true,
    );
    support::wait_real(cx, "the archive", |cx| {
        support::present(cx, win, ids::store_file(0))
    });
    support::dialog_box(cx, win);
    let bar = ids::STORE_DETAIL_SCROLLBAR;
    assert_bar_beside(cx, win, ids::STORE_DETAIL_BODY, bar);
    assert_clear_of_bar(cx, win, ids::store_file(0), bar);
    let close = cx
        .update_window(win, |_, w, _| {
            w.within("dialog").try_find("close").map(|e| e.bounds())
        })
        .unwrap()
        .expect("the close X");
    let track = bounds(cx, win, bar);
    assert!(
        close.bottom() <= track.top(),
        "the X {close:?} sits over the bar {track:?}"
    );
    let name = bounds(cx, win, ids::STORE_DETAIL_NAME);
    assert_track_scrolls(cx, win, bar, ids::store_file(0));
    assert_eq!(
        bounds(cx, win, ids::STORE_DETAIL_NAME),
        name,
        "the header stays"
    );
}
