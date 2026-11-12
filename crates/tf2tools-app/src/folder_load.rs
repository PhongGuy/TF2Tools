//! A value read once per found TF2 `tf` folder on the background pool, such as the game's
//! weapon names or kill icons. Each value type has its own session global.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use gpui_kit::{App, AppContext as _, Context, Entity, Global, Subscription, Task};
use tf2tools_core::Tf2Install;

use crate::services::{Services, Tf2Global, Tf2State, run_blocking};

/// Reads a `tf` folder's value on the background pool.
pub type FolderLoader<T> = Arc<dyn Fn(&Services, &Path) -> T + Send + Sync>;

/// The session's value of one kind, loaded once per found `tf` folder.
pub struct FolderLoad<T> {
    shown_for: Option<PathBuf>,
    value: T,
    loads: usize,
    loader: FolderLoader<T>,
    task: Option<Task<()>>,
    _tf2: Option<Subscription>,
}

// One global per value type, so two kinds of load never collide.
struct FolderGlobal<T: 'static>(Entity<FolderLoad<T>>);
impl<T: 'static> Global for FolderGlobal<T> {}

impl<T: Default + Send + 'static> FolderLoad<T> {
    /// The session's load of `T`, created on `make`'s loader on first use. Needs `Services`.
    pub fn global_or(cx: &mut App, make: impl FnOnce() -> FolderLoader<T>) -> Entity<Self> {
        match cx.try_global::<FolderGlobal<T>>() {
            Some(global) => global.0.clone(),
            None => Self::install_with(cx, make()),
        }
    }

    /// Installs the session's load on `loader` and sets the global. When a `Tf2Global` is
    /// installed it follows that state now and on every change; without one nothing loads.
    pub fn install_with(cx: &mut App, loader: FolderLoader<T>) -> Entity<Self> {
        let tf2 = cx.try_global::<Tf2Global>().map(|g| g.0.clone());
        let load = cx.new(|cx| {
            let mut me = Self {
                shown_for: None,
                value: T::default(),
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
        cx.set_global(FolderGlobal(load.clone()));
        load
    }

    /// The value shown now: the default before a load lands or without a game.
    pub fn value(&self) -> &T {
        &self.value
    }

    /// Loads started this session.
    pub fn loads(&self) -> usize {
        self.loads
    }

    /// Whether a started load has yet to land.
    pub fn loading(&self) -> bool {
        self.task.is_some()
    }

    /// Shows `tf_dir`'s value, loading it unless it is shown or loading already; None clears.
    pub fn show_game(&mut self, tf_dir: Option<PathBuf>, cx: &mut Context<Self>) {
        if tf_dir == self.shown_for {
            return;
        }
        self.value = T::default();
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
            // A panicking loader leaves the default, like a failed read.
            let value = loading.await.unwrap_or_default();
            let _ = this.update(cx, |this, cx| {
                // Backstop: a newer show_game drops this task, but never apply another folder's value.
                if this.shown_for.as_deref() == Some(dir.as_path()) {
                    this.value = value;
                    this.task = None;
                    cx.notify();
                }
            });
        }));
    }
}

fn tf_dir(state: &Tf2State) -> Option<PathBuf> {
    state.install().map(Tf2Install::tf_dir)
}
