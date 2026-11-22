//! Driving the Graphics page's settings list.

use gpui_kit::component::select::SelectEvent;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AppContext as _, ElementId, Entity, ScrollDelta, TestAppContext, point};
use tf2tools_app::pages::graphics::GraphicsPage;
use tf2tools_app::pages::graphics::model::ValueOption;
use tf2tools_app::services::Tf2State;
use tf2tools_app::shell::route::Page;
use tf2tools_core::{ModuleChoice, Tf2Install};

/// The first line of every modules.cfg TF2Tools writes.
pub const HEADER: &str = "// Written by TF2Tools; changes here are replaced\n";

pub fn found(env: &super::Env) -> Tf2State {
    Tf2State::Found(Tf2Install::from_path(&env.game).unwrap())
}

pub fn value(v: &str) -> ModuleChoice {
    ModuleChoice::Value(v.to_owned())
}

/// A shell on a found TF2 whose store talks to `server`.
pub fn server_shell(
    cx: &mut TestAppContext,
    env: &super::Env,
    server: &super::StoreServer,
) -> super::ShellWindow {
    cx.executor().allow_parking();
    let endpoints = server.endpoints();
    super::shell_window_tweaked(cx, env, found(env), |services| {
        *services = services.clone().with_endpoints(endpoints);
    })
}

/// Opens Graphics from the sidebar and waits for its first load. A missing catalog is
/// fetched on the tokio bridge, so the executor must be allowed to park.
pub fn open(cx: &mut TestAppContext, s: &super::ShellWindow) -> Entity<GraphicsPage> {
    cx.executor().allow_parking();
    super::click_nav(cx, s.win, Page::Graphics);
    let page = cx
        .update(|cx| s.shell.read(cx).graphics_page())
        .expect("a Graphics page");
    super::wait_real(cx, "Graphics to load", |cx| {
        cx.update(|cx| page.read(cx).is_loaded())
    });
    super::settle(cx, s.win);
    page
}

/// Waits until the page holds its catalog, so every row and its Select is drawn.
pub fn wait_for_list(cx: &mut TestAppContext, s: &super::ShellWindow, page: &Entity<GraphicsPage>) {
    super::wait_real(cx, "the settings list", |cx| {
        cx.update(|cx| page.read(cx).catalog().is_some())
    });
    super::settle(cx, s.win);
}

/// The value `module`'s Select shows.
pub fn shown(
    cx: &mut TestAppContext,
    page: &Entity<GraphicsPage>,
    module: &str,
) -> Option<ModuleChoice> {
    cx.update(|cx| {
        let select = page.read(cx).module_select(module)?;
        select.read(cx).selected_value().cloned()
    })
}

/// The value `module`'s row shows, whatever its control.
pub fn row_value(
    cx: &mut TestAppContext,
    page: &Entity<GraphicsPage>,
    module: &str,
) -> Option<ModuleChoice> {
    cx.update(|cx| page.read(cx).row(module).map(|r| r.shown))
}

/// The page's saved choice for `module`.
pub fn saved(
    cx: &mut TestAppContext,
    page: &Entity<GraphicsPage>,
    module: &str,
) -> Option<ModuleChoice> {
    cx.update(|cx| page.read(cx).choices().and_then(|c| c.get(module)).cloned())
}

/// Picks `choice` in `module`'s Select as a click would; None is a cleared Select.
pub fn choose(
    cx: &mut TestAppContext,
    page: &Entity<GraphicsPage>,
    module: &str,
    choice: Option<ModuleChoice>,
) {
    let select = cx
        .update(|cx| page.read(cx).module_select(module))
        .unwrap_or_else(|| panic!("no row for {module}"));
    cx.update(|cx| {
        select.update(cx, |_, cx| {
            cx.emit(SelectEvent::<Vec<ValueOption>>::Confirm(choice))
        })
    });
}

/// Picks `choice` in `module`'s Select as a click does: the Select shows it, then confirms it.
pub fn pick(
    cx: &mut TestAppContext,
    s: &super::ShellWindow,
    page: &Entity<GraphicsPage>,
    module: &str,
    choice: ModuleChoice,
) {
    let select = cx
        .update(|cx| page.read(cx).module_select(module))
        .unwrap_or_else(|| panic!("no row for {module}"));
    cx.update_window(s.win, |_, window, cx| {
        select.update(cx, |select, cx| {
            select.set_selected_value(&choice, window, cx)
        })
    })
    .unwrap();
    choose(cx, page, module, Some(choice));
}

/// Scrolls the Graphics page down a line at a time until `id`'s centre, where a click lands,
/// is inside the page; panics if it never is.
pub fn reveal(cx: &mut TestAppContext, s: &super::ShellWindow, id: ElementId) {
    let area = tf2tools_app::ids::page(Page::Graphics);
    // Enough lines to reach the last category of the full catalogue with every row carded.
    for _ in 0..400 {
        let seen = cx
            .update_window(s.win, |_, w, _| {
                let page = w.try_find(area.clone()).map(|e| e.bounds());
                w.try_find(id.clone()).is_some_and(|e| {
                    e.visible() && page.is_some_and(|page| page.contains(&e.bounds().center()))
                })
            })
            .unwrap();
        if seen {
            return;
        }
        let down = ScrollDelta::Lines(point(0., -1.));
        cx.update_window(s.win, |_, w, cx| w.scroll(area.clone(), down, cx))
            .unwrap();
        super::settle(cx, s.win);
    }
    panic!("{id:?} never showed on the Graphics page");
}

/// Opens or closes `module`'s pictures with a click on its thumbnail.
pub fn toggle_pictures(cx: &mut TestAppContext, s: &super::ShellWindow, module: &str) {
    reveal(cx, s, tf2tools_app::ids::graphics_thumb(module));
    super::click_id(cx, s.win, tf2tools_app::ids::graphics_thumb(module));
}

/// The distinct colours of the borders the last frame painted exactly around `id`'s box,
/// leaving out borders drawn transparent or zero wide. A kit Button paints several identical
/// quads over its box, so each colour is listed once.
pub fn borders_at(
    cx: &mut TestAppContext,
    s: &super::ShellWindow,
    id: impl Into<ElementId>,
) -> Vec<gpui_kit::Hsla> {
    let id = id.into();
    let b = super::bounds_of(cx, s.win, id.clone()).unwrap_or_else(|| panic!("{id:?} not drawn"));
    cx.update_window(s.win, |_, w, _| {
        let scale = w.scale_factor();
        let near = |a: f32, b: gpui_kit::Pixels| (a - f32::from(b) * scale).abs() <= 0.5;
        w.painted_quads()
            .iter()
            .filter(|q| {
                near(q.bounds.origin.x.0, b.origin.x)
                    && near(q.bounds.origin.y.0, b.origin.y)
                    && near(q.bounds.size.width.0, b.size.width)
                    && near(q.bounds.size.height.0, b.size.height)
                    && q.border_widths.top.0 > 0.
            })
            .map(|q| q.border_color)
            .filter(|c| c.a > 0.)
            .fold(Vec::new(), |mut seen, c| {
                if !seen.contains(&c) {
                    seen.push(c);
                }
                seen
            })
    })
    .unwrap()
}

/// Scrolls the Graphics page back to its top, where the status card and changes strip are.
pub fn scroll_to_top(cx: &mut TestAppContext, s: &super::ShellWindow) {
    let area = tf2tools_app::ids::page(Page::Graphics);
    let up = ScrollDelta::Lines(point(0., 10_000.));
    cx.update_window(s.win, |_, w, cx| w.scroll(area, up, cx))
        .unwrap();
    super::settle(cx, s.win);
}

/// Types `text` into the settings search as the player would.
pub fn search(
    cx: &mut TestAppContext,
    s: &super::ShellWindow,
    page: &Entity<GraphicsPage>,
    text: &str,
) {
    let input = cx.update(|cx| page.read(cx).search_input().clone());
    cx.update_window(s.win, |_, window, cx| {
        input.update(cx, |st, cx| st.set_value(text, window, cx))
    })
    .unwrap();
    super::settle(cx, s.win);
}

/// Asserts every row's control lies inside its row and the window, after the window is made
/// `width` px wide; the page needs the full catalogue on screen.
pub fn assert_controls_fit(cx: &mut TestAppContext, s: &super::ShellWindow, width: f32) {
    use tf2tools_app::pages::graphics::view::{ControlKind, control_kind};
    cx.simulate_window_resize(
        s.win,
        gpui_kit::size(gpui_kit::px(width), gpui_kit::px(900.)),
    );
    super::settle(cx, s.win);
    let catalog = super::full_module_catalog();
    for module in catalog.categories().iter().flat_map(|c| c.modules()) {
        let name = module.name();
        let control = match control_kind(module) {
            ControlKind::Switch => tf2tools_app::ids::graphics_switch(name),
            ControlKind::Segmented => tf2tools_app::ids::graphics_segments(name),
            ControlKind::Select => tf2tools_app::ids::graphics_module_select(name),
        };
        let row = super::bounds_of(cx, s.win, tf2tools_app::ids::graphics_row(name))
            .unwrap_or_else(|| panic!("no row for {name}"));
        let c =
            super::bounds_of(cx, s.win, control).unwrap_or_else(|| panic!("no control for {name}"));
        assert!(
            c.left() >= row.left() && c.right() <= row.right(),
            "{name}'s control {c:?} spills out of its row {row:?}"
        );
        assert!(
            c.right() <= gpui_kit::px(width) && c.size.width > gpui_kit::px(0.),
            "{name}'s control {c:?} is cut off at {width} px"
        );
    }
}

/// At `width`, asserts the empty strip keeps the reserved `height`, its text stays inside the
/// strip's side padding and border, and sits vertically centred.
pub fn assert_as_is_text_fits(
    cx: &mut TestAppContext,
    s: &super::ShellWindow,
    width: f32,
    height: f32,
) {
    use gpui_kit::px;
    cx.simulate_window_resize(s.win, gpui_kit::size(px(width), px(900.)));
    super::settle(cx, s.win);
    let ids = (
        tf2tools_app::ids::GRAPHICS_NO_CHANGES,
        tf2tools_app::ids::GRAPHICS_NO_CHANGES_TEXT,
    );
    let strip = super::bounds_of(cx, s.win, ids.0).expect("the empty strip");
    let text = super::bounds_of(cx, s.win, ids.1).expect("the empty strip's text");
    // The strip's 1 px border and its px_4 side padding.
    let (side, border) = (px(17.), px(1.));
    assert_eq!(
        strip.size.height,
        px(height),
        "the strip keeps its height: {strip:?}"
    );
    assert!(
        text.left() >= strip.left() + side && text.right() <= strip.right() - side,
        "the text {text:?} runs out of the strip {strip:?} at {width} px"
    );
    let (above, below) = (text.top() - strip.top(), strip.bottom() - text.bottom());
    assert!(
        above >= border && below >= border,
        "the text {text:?} overflows the strip's height {strip:?} at {width} px"
    );
    assert!(
        (above - below).abs() <= px(1.),
        "the text {text:?} is not centred in the strip {strip:?} at {width} px"
    );
}

pub fn idle(cx: &mut TestAppContext, page: &Entity<GraphicsPage>) {
    super::wait_real(cx, "the page to settle", |cx| {
        cx.update(|cx| !page.read(cx).is_busy())
    });
}

pub fn toasted(cx: &mut TestAppContext, s: &super::ShellWindow, text: &str) {
    super::wait_real(cx, text, |cx| {
        super::toast_labels(cx, s.win)
            .iter()
            .any(|t| t.contains(text))
    });
}

/// tf/cfg/overrides/modules.cfg, if there is one.
pub fn modules_cfg(env: &super::Env) -> Option<String> {
    std::fs::read_to_string(env.game.join("tf/cfg/overrides/modules.cfg")).ok()
}

/// Waits until the page has read the bundled pictures, found or not.
pub fn wait_for_pictures(
    cx: &mut TestAppContext,
    s: &super::ShellWindow,
    page: &Entity<GraphicsPage>,
) {
    super::wait_real(cx, "the pictures", |cx| {
        cx.update(|cx| page.read(cx).pictures().is_some())
    });
    super::settle(cx, s.win);
}

/// A full-catalogue page on `preset` with `choices` saved, its list and pictures loaded.
pub fn picture_page(
    cx: &mut TestAppContext,
    env: &super::Env,
    preset: &str,
    choices: &str,
) -> (super::ShellWindow, Entity<GraphicsPage>) {
    let s = super::shell_window(cx, env, found(env));
    super::seed_managed_mastercomfig(env, "9.100.1", preset);
    super::seed_full_mastercomfig_catalog(env, "9.100.1");
    super::seed_mastercomfig_choices(env, choices);
    let page = open(cx, &s);
    wait_for_list(cx, &s, &page);
    wait_for_pictures(cx, &s, &page);
    (s, page)
}

/// The key URL of `<setting-pictures>/<module>/<file>` as the page builds it.
pub fn picture_url(
    cx: &mut TestAppContext,
    s: &super::ShellWindow,
    page: &Entity<GraphicsPage>,
    module: &str,
    file: &str,
) -> String {
    let path = s
        .services
        .assets
        .setting_pictures_dir()
        .join(module)
        .join(file);
    cx.update(|cx| page.read(cx).pictures().expect("pictures").url(&path))
}

/// Whether the page's last frame asked the image cache for `key`.
pub fn wants(
    cx: &mut TestAppContext,
    page: &Entity<GraphicsPage>,
    key: &tf2tools_app::remote_images::ImageKey,
) -> bool {
    cx.update(|cx| page.read(cx).wanted_images().contains(key))
}

/// An asset root holding only the crosshair file `AssetRoot` checks for.
pub fn bare_assets(env: &super::Env) -> std::path::PathBuf {
    let root = env.tmp.path().join("bare-assets");
    std::fs::create_dir_all(root.join("crosshairs")).unwrap();
    std::fs::write(root.join("crosshairs/crosshairs.json"), "[]").unwrap();
    root
}

/// A bare asset root whose pictures folder holds only lod (low, medium) with `medium` as given.
pub fn lod_only_assets(env: &super::Env, medium: &[u8]) -> tf2tools_app::assets::AssetRoot {
    let root = bare_assets(env);
    let pictures = root.join("setting-pictures");
    std::fs::create_dir_all(pictures.join("lod")).unwrap();
    std::fs::write(
        pictures.join("manifest.json"),
        r#"{"version": 1, "mastercomfig": "9.100.1", "size": [960, 540], "map": "itemtest", "base": {},
           "settings": [{"module": "lod", "values": [{"value": "low"}, {"value": "medium"}]}]}"#,
    )
    .unwrap();
    std::fs::write(pictures.join("lod/medium.webp"), medium).unwrap();
    tf2tools_app::assets::AssetRoot::resolve_from(Some(root), None, None).unwrap()
}

/// Fails every network fetch; the page's pictures are all on disk.
pub fn no_network() -> tf2tools_app::remote_images::ImageFetch {
    std::sync::Arc::new(|_url: String| {
        Box::pin(async { Err(tf2tools_app::remote_images::FetchFailure::Permanent) })
    })
}

/// Whether `module`'s `value` segment is checked; None when the row draws no such segment.
pub fn segment_checked(
    cx: &mut TestAppContext,
    s: &super::ShellWindow,
    module: &str,
    value: &str,
) -> Option<bool> {
    super::checked_of(
        cx,
        s.win,
        tf2tools_app::ids::graphics_segment(module, value),
    )
}

/// Clicks `module`'s `value` segment as the player would.
pub fn click_segment(cx: &mut TestAppContext, s: &super::ShellWindow, module: &str, value: &str) {
    reveal(cx, s, tf2tools_app::ids::graphics_segment(module, value));
    super::click_id(
        cx,
        s.win,
        tf2tools_app::ids::graphics_segment(module, value),
    );
}

/// Asserts each changed `module`'s preset note, Reset and segments lie inside its row and the
/// `width` px window, under the row's text, on a page already `width` px wide.
pub fn assert_changed_rows_fit(
    cx: &mut TestAppContext,
    s: &super::ShellWindow,
    modules: &[&str],
    width: f32,
) {
    use tf2tools_app::ids;
    for &name in modules {
        let found = |cx: &mut TestAppContext, part: &str, id: ElementId| {
            super::bounds_of(cx, s.win, id).unwrap_or_else(|| panic!("no {part} for {name}"))
        };
        let row = found(cx, "row", ids::graphics_row(name));
        let more = found(cx, "Learn more", ids::graphics_learn_more(name));
        for (part, id) in [
            ("note", ids::graphics_module_preset(name)),
            ("Reset", ids::graphics_module_reset(name)),
            ("control", ids::graphics_segments(name)),
        ] {
            let b = found(cx, part, id);
            assert!(
                b.left() >= row.left() && b.right() <= row.right(),
                "{name}'s {part} {b:?} spills out of its row {row:?}"
            );
            assert!(
                b.right() <= gpui_kit::px(width) && b.size.width > gpui_kit::px(0.),
                "{name}'s {part} {b:?} is cut off at {width} px"
            );
            assert!(
                b.top() >= more.bottom(),
                "{name}'s {part} {b:?} sits under the row's text {more:?}"
            );
        }
    }
}
