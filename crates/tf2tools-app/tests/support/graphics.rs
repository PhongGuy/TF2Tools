//! Driving the Graphics page's settings list.

use gpui_kit::component::select::SelectEvent;
use gpui_kit::{Entity, TestAppContext};
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
