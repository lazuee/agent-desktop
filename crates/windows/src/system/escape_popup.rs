//! Closing the open popup of a window that does not hold the keyboard.
//!
//! Core asks for it after a plain Escape that the key route refused for focus.
//! A popup a person opened (a menu, a drop-down, a split-button list) can sit
//! in a window that is not in front. It has a semantic close: collapsing its
//! owner, through the same primitive `collapse` uses (`disclosure_steps`).

#[cfg(not(target_os = "windows"))]
use agent_desktop_core::{
    ActionResult, AdapterError, Deadline, InteractionPolicy, ProcessIdentity, WindowInfo,
};

/// The control types whose expansion is a popup: menu item (50011), combo box
/// (50003) and split button (50031). A tree item or a group is never one.
fn opens_a_popup(control_type: i32) -> bool {
    [50011, 50003, 50031].contains(&control_type)
}

#[cfg(target_os = "windows")]
#[path = "escape_popup_uia.rs"]
mod uia;

#[cfg(target_os = "windows")]
pub(crate) use uia::close_transient_surface;

/// Off Windows no popup can be open.
#[cfg(not(target_os = "windows"))]
pub(crate) fn close_transient_surface(
    _process: &ProcessIdentity,
    _window: Option<&WindowInfo>,
    _policy: InteractionPolicy,
    _deadline: Deadline,
) -> Result<Option<ActionResult>, AdapterError> {
    Ok(None)
}

#[cfg(test)]
#[path = "escape_popup_tests.rs"]
mod tests;
