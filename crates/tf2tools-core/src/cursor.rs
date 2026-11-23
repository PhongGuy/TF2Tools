//! The system cursor calls a pointer lock needs: hide, show, read and warp. On Windows they go
//! to user32; everywhere else they do nothing, read `None` and report a failed warp.
//!
//! `ShowCursor` keeps a per-thread display counter, not a flag: every `hide` must be matched by
//! one `show` on the same thread, or the cursor stays hidden after the lock ends.

/// Hides the cursor by lowering this thread's display counter (`ShowCursor(FALSE)`).
pub fn hide() {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::FALSE;
        // SAFETY: ShowCursor takes no pointers and only adjusts a counter.
        #[allow(unsafe_code)]
        let _counter = unsafe { windows_sys::Win32::UI::WindowsAndMessaging::ShowCursor(FALSE) };
    }
}

/// Shows the cursor again by raising this thread's display counter (`ShowCursor(TRUE)`).
pub fn show() {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::TRUE;
        // SAFETY: ShowCursor takes no pointers and only adjusts a counter.
        #[allow(unsafe_code)]
        let _counter = unsafe { windows_sys::Win32::UI::WindowsAndMessaging::ShowCursor(TRUE) };
    }
}

/// The cursor's screen position in physical pixels, or `None` when it cannot be read (no
/// desktop, a locked session, or any platform but Windows).
pub fn position() -> Option<(i32, i32)> {
    #[cfg(windows)]
    {
        let mut point = windows_sys::Win32::Foundation::POINT { x: 0, y: 0 };
        // SAFETY: `point` is a valid, writable POINT that outlives the call.
        #[allow(unsafe_code)]
        let ok = unsafe { windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut point) };
        (ok != 0).then_some((point.x, point.y))
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// Moves the cursor to screen position `x`, `y` in physical pixels; `false` when the move
/// failed or the platform is not Windows.
pub fn set_position(x: i32, y: i32) -> bool {
    #[cfg(windows)]
    {
        // SAFETY: SetCursorPos takes plain integers.
        #[allow(unsafe_code)]
        let ok = unsafe { windows_sys::Win32::UI::WindowsAndMessaging::SetCursorPos(x, y) };
        ok != 0
    }
    #[cfg(not(windows))]
    {
        let _ = (x, y);
        false
    }
}

#[cfg(test)]
mod tests {
    #[cfg(not(windows))]
    #[test]
    fn off_windows_the_cursor_calls_do_nothing() {
        super::hide();
        super::show();
        assert_eq!(super::position(), None, "no cursor position off Windows");
        assert!(!super::set_position(10, 10), "no warp off Windows");
    }

    #[cfg(windows)]
    #[test]
    fn on_windows_a_warp_reads_back() {
        // CI desktops have a cursor; a session without one reads None and the test has nothing to check.
        let Some(start) = super::position() else {
            return;
        };
        assert!(
            super::set_position(start.0 + 1, start.1 + 1),
            "SetCursorPos failed"
        );
        assert_eq!(
            super::position(),
            Some((start.0 + 1, start.1 + 1)),
            "warp did not read back"
        );
        assert!(super::set_position(start.0, start.1), "restore failed");
        super::hide();
        super::show();
    }
}
