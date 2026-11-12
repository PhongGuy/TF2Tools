//! The session's game weapon names for logs.tf: the found TF2's English names, read once per
//! `tf` folder on the background pool.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui_kit::{App, AppContext as _, Context, Entity, Global, Subscription, Task};
use tf2tools_core::{Tf2Install, WeaponNames};

use crate::services::{Services, Tf2Global, Tf2State, run_blocking};

/// Reads a `tf` folder's weapon names on the background pool.
pub type NamesLoader = Arc<dyn Fn(&Services, &Path) -> WeaponNames + Send + Sync>;

// The production loader: core's read; a failed read leaves every log name raw.
fn read_names(_: &Services, tf_dir: &Path) -> WeaponNames {
    WeaponNames::load(tf_dir).unwrap_or_else(|err| {
        tracing::warn!(%err, "could not read the game's weapon names");
        WeaponNames::default()
    })
}

/// The session's weapon names, loaded once per found `tf` folder.
pub struct LoadedWeaponNames {
    shown_for: Option<PathBuf>,
    names: WeaponNames,
    loads: usize,
    loader: NamesLoader,
    task: Option<Task<()>>,
    _tf2: Option<Subscription>,
}

struct WeaponNamesGlobal(Entity<LoadedWeaponNames>);
impl Global for WeaponNamesGlobal {}

impl LoadedWeaponNames {
    /// The session's names on the production loader, created on first use. Needs `Services`.
    pub fn global(cx: &mut App) -> Entity<Self> {
        match cx.try_global::<WeaponNamesGlobal>() {
            Some(global) => global.0.clone(),
            None => Self::install_with(cx, Arc::new(read_names)),
        }
    }

    /// Installs the session's names on `loader` and sets the global. When a `Tf2Global` is
    /// installed it follows that state now and on every change; without one nothing loads.
    pub fn install_with(cx: &mut App, loader: NamesLoader) -> Entity<Self> {
        let tf2 = cx.try_global::<Tf2Global>().map(|g| g.0.clone());
        let names = cx.new(|cx| {
            let mut me = Self {
                shown_for: None,
                names: WeaponNames::default(),
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
        cx.set_global(WeaponNamesGlobal(names.clone()));
        names
    }

    /// The names shown now: empty before a load lands or without a game.
    pub fn names(&self) -> &WeaponNames {
        &self.names
    }

    /// Loads started this session.
    pub fn loads(&self) -> usize {
        self.loads
    }

    /// Shows `tf_dir`'s names, loading them unless they are shown or loading already; None clears.
    pub fn show_game(&mut self, tf_dir: Option<PathBuf>, cx: &mut Context<Self>) {
        if tf_dir == self.shown_for {
            return;
        }
        self.names = WeaponNames::default();
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
            // A panicking loader leaves the log names raw, like a failed read.
            let names = loading.await.unwrap_or_default();
            let _ = this.update(cx, |this, cx| {
                // Backstop: a newer show_game drops this task, but never apply another folder's names.
                if this.shown_for.as_deref() == Some(dir.as_path()) {
                    this.names = names;
                    cx.notify();
                }
            });
        }));
    }
}

fn tf_dir(state: &Tf2State) -> Option<PathBuf> {
    state.install().map(Tf2Install::tf_dir)
}
