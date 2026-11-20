//! Whether an application shows an open menu or popover. AppKit draws both in
//! a window at the pop-up menu level, and the accessibility window list
//! leaves that window out.

use super::cg_window::{bool_field, int_field, window_dictionaries};

/// `kCGPopUpMenuWindowLevel`.
const POPUP_MENU_WINDOW_LEVEL: i64 = 101;

fn is_open_popup(layer: Option<i64>, on_screen: bool) -> bool {
    layer == Some(POPUP_MENU_WINDOW_LEVEL) && on_screen
}

/// Whether `pid` has an on-screen window at the pop-up menu level. A window
/// list that cannot be read says yes, so a caller searches instead of
/// missing a surface.
pub(crate) fn shows_popup_window(pid: i32) -> bool {
    let Ok(windows) = window_dictionaries() else {
        return true;
    };
    windows.iter().any(|window| {
        int_field(window, "kCGWindowOwnerPID") == Some(i64::from(pid))
            && is_open_popup(
                int_field(window, "kCGWindowLayer"),
                bool_field(window, "kCGWindowIsOnscreen").unwrap_or(true),
            )
    })
}

#[cfg(test)]
#[path = "popup_window_tests.rs"]
mod tests;
