//! The session's game weapon icons: the found TF2's backpack pictures, built into cached
//! PNGs once per `tf` folder on the background pool and drawn by path.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui_kit::{App, AppContext as _, Context, Entity, Global, Subscription, Task};
use tf2tools_core::{GameWeaponIcons, Tf2Install, weapons};

use crate::services::{Services, Tf2Global, Tf2State, run_blocking};

/// Builds a `tf` folder's icons on the background pool: weapon script to cached PNG.
pub type IconLoader = Arc<dyn Fn(&Services, &Path) -> HashMap<&'static str, PathBuf> + Send + Sync>;

// The production loader: core's cache build; a failed build shows slot squares.
fn build_icons(s: &Services, tf_dir: &Path) -> HashMap<&'static str, PathBuf> {
    match GameWeaponIcons::build(tf_dir, &s.data.weapon_icon_dir()) {
        Ok(built) => weapons()
            .iter()
            .filter_map(|w| Some((w.script_name(), built.get(w.script_name())?.to_path_buf())))
            .collect(),
        Err(err) => {
            tracing::warn!(%err, "could not build the game's weapon icons");
            HashMap::new()
        }
    }
}

/// The session's game weapon icons, loaded once per found `tf` folder.
pub struct WeaponIcons {
    shown_for: Option<PathBuf>,
    icons: HashMap<&'static str, PathBuf>,
    loads: usize,
    loader: IconLoader,
    task: Option<Task<()>>,
    _tf2: Option<Subscription>,
}

struct WeaponIconsGlobal(Entity<WeaponIcons>);
impl Global for WeaponIconsGlobal {}

impl WeaponIcons {
    /// The session's icons on the production loader, created on first use. Needs `Services`.
    pub fn global(cx: &mut App) -> Entity<Self> {
        match cx.try_global::<WeaponIconsGlobal>() {
            Some(global) => global.0.clone(),
            None => Self::install_with(cx, Arc::new(build_icons)),
        }
    }

    /// Installs the session's icons on `loader` and sets the global. When a `Tf2Global` is
    /// installed it follows that state now and on every change; without one nothing loads.
    pub fn install_with(cx: &mut App, loader: IconLoader) -> Entity<Self> {
        let tf2 = cx.try_global::<Tf2Global>().map(|g| g.0.clone());
        let icons = cx.new(|cx| {
            let mut me = Self {
                shown_for: None,
                icons: HashMap::new(),
                loads: 0,
                loader,
                task: None,
                _tf2: None,
            };
            if let Some(tf2) = tf2 {
                me._tf2 = Some(cx.observe(&tf2, |me: &mut Self, tf2, cx| {
                    me.show_game(tf_dir(tf2.read(cx)), cx);
                }));
                me.show_game(tf_dir(tf2.read(cx)), cx);
            }
            me
        });
        cx.set_global(WeaponIconsGlobal(icons.clone()));
        icons
    }

    /// The cached PNG to draw for `script`, None before its load lands or when the game lacks it.
    pub fn icon(&self, script: &str) -> Option<&Path> {
        self.icons.get(script).map(PathBuf::as_path)
    }

    /// Loads started this session.
    pub fn loads(&self) -> usize {
        self.loads
    }

    /// Shows `tf_dir`'s icons, loading them unless they are shown or loading already; None clears.
    pub fn show_game(&mut self, tf_dir: Option<PathBuf>, cx: &mut Context<Self>) {
        if tf_dir == self.shown_for {
            return;
        }
        self.icons.clear();
        self.task = None;
        self.shown_for = tf_dir.clone();
        cx.notify();
        let Some(dir) = tf_dir else {
            return;
        };
        self.loads += 1;
        let loader = Arc::clone(&self.loader);
        let folder = dir.clone();
        let loading = run_blocking(cx, move |s| Ok(loader(s, &folder)));
        self.task = Some(cx.spawn(async move |this, cx| {
            // A panicking loader leaves slot squares, like a failed build.
            let icons = loading.await.unwrap_or_default();
            let _ = this.update(cx, |this, cx| this.land(&dir, icons, cx));
        }));
    }

    /// Applies a finished load for `tf_dir`; ignored unless `tf_dir` is still the folder shown,
    /// so a load for a folder TF2 has since left never overwrites the newer one.
    pub fn land(
        &mut self,
        tf_dir: &Path,
        icons: HashMap<&'static str, PathBuf>,
        cx: &mut Context<Self>,
    ) {
        if self.shown_for.as_deref() != Some(tf_dir) {
            return;
        }
        self.icons = icons;
        cx.notify();
    }
}

fn tf_dir(state: &Tf2State) -> Option<PathBuf> {
    state.install().map(Tf2Install::tf_dir)
}
