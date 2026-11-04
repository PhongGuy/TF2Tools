//! The app-wide "library changed" signal: every operation that changes the library or what is
//! installed from it emits here, and the library pages reload on it.

use gpui_kit::{App, AppContext as _, Entity, EventEmitter, Global};

/// Emitted after anything changes the library or what is installed from it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LibraryChanged;

/// The stateless entity that carries `LibraryChanged` to its subscribers.
pub struct LibraryEvents;

impl EventEmitter<LibraryChanged> for LibraryEvents {}

struct LibraryEventsGlobal(Entity<LibraryEvents>);

impl Global for LibraryEventsGlobal {}

impl LibraryEvents {
    /// The app's instance, installed on first use.
    pub fn global(cx: &mut App) -> Entity<Self> {
        if let Some(existing) = cx.try_global::<LibraryEventsGlobal>() {
            return existing.0.clone();
        }
        let events = cx.new(|_| LibraryEvents);
        cx.set_global(LibraryEventsGlobal(events.clone()));
        events
    }

    /// Emits `LibraryChanged` on the app's instance.
    pub fn changed(cx: &mut App) {
        Self::global(cx).update(cx, |_, cx| cx.emit(LibraryChanged));
    }
}
