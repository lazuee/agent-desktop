//! Closing the open menu or popover of an application that does not hold the
//! keyboard. Core asks for it after a plain Escape that the key route refused
//! for focus: the surface can stay open in a window that does not.

use std::time::Duration;

use agent_desktop_core::{
    Action, ActionResult, ActionStep, AdapterError, Deadline, ErrorCode, InteractionLease,
    InteractionPolicy, KeyCombo, ProcessIdentity, StepMechanism, WindowInfo,
};

use crate::tree::AXElement;

pub(crate) const CANCEL: &str = "AXCancel";
const SURFACE_CLOSE_WAIT: Duration = Duration::from_millis(500);
const SURFACE_CLOSE_STEP: Duration = Duration::from_millis(10);

/// The reads and the one action a close needs, so the routing between a named
/// window and the whole process is testable without a live application.
pub(crate) trait TransientSurface {
    type Surface;

    fn in_window(&self, window: &WindowInfo) -> Result<Option<Self::Surface>, AdapterError>;

    fn in_process(&self, process: &ProcessIdentity) -> Result<Option<Self::Surface>, AdapterError>;

    fn cancel(&self, surface: &Self::Surface) -> Result<bool, AdapterError>;

    fn closed(&self, surface: &Self::Surface) -> bool;
}

struct AxSurfaces {
    deadline: Deadline,
}

impl TransientSurface for AxSurfaces {
    type Surface = AXElement;

    fn in_window(&self, window: &WindowInfo) -> Result<Option<AXElement>, AdapterError> {
        let element =
            crate::system::window::window_resolve::window_element_for_info(window, self.deadline)?;
        crate::tree::surfaces::transient_surface_in_window(&element, self.deadline)
    }

    fn in_process(&self, process: &ProcessIdentity) -> Result<Option<AXElement>, AdapterError> {
        let pid = crate::system::process::process_identity::to_pid_t(process.pid)?;
        crate::tree::surfaces::transient_surface_for_pid(pid, self.deadline)
    }

    fn cancel(&self, surface: &AXElement) -> Result<bool, AdapterError> {
        crate::actions::ax_helpers::try_ax_action_or_err(surface, CANCEL, self.deadline)
    }

    fn closed(&self, surface: &AXElement) -> bool {
        surface_closed(surface, self.deadline)
    }
}

/// Closes the surface `window` shows, or the one the process shows anywhere
/// when `window` shows none or is absent. `None` when none is open.
pub(crate) fn close_transient_surface(
    process: &ProcessIdentity,
    window: Option<&WindowInfo>,
    _policy: InteractionPolicy,
    lease: &InteractionLease,
) -> Result<Option<ActionResult>, AdapterError> {
    close_with(
        &AxSurfaces {
            deadline: lease.deadline(),
        },
        process,
        window,
    )
}

/// The close of one surface through its own `AXCancel`, verified by reading it
/// gone. `None` when the surface does not advertise the action.
pub(crate) fn cancel_surface(
    surface: &AXElement,
    deadline: Deadline,
) -> Result<Option<ActionStep>, AdapterError> {
    if !crate::actions::ax_helpers::try_ax_action_or_err(surface, CANCEL, deadline)? {
        return Ok(None);
    }
    Ok(Some(action_step(surface_closed(surface, deadline))))
}

fn close_with<S: TransientSurface>(
    surfaces: &S,
    process: &ProcessIdentity,
    window: Option<&WindowInfo>,
) -> Result<Option<ActionResult>, AdapterError> {
    let Some(surface) = find_surface(surfaces, process, window)? else {
        return Ok(None);
    };
    if !surfaces.cancel(&surface)? {
        return Ok(None);
    }
    Ok(Some(closed_result(action_step(surfaces.closed(&surface)))))
}

/// The surface inside the named window when that window shows one, otherwise
/// the surface the process shows. Only the process is read without a window.
fn find_surface<S: TransientSurface>(
    surfaces: &S,
    process: &ProcessIdentity,
    window: Option<&WindowInfo>,
) -> Result<Option<S::Surface>, AdapterError> {
    let Some(window) = window else {
        return surfaces.in_process(process);
    };
    match surfaces.in_window(window)? {
        Some(surface) => Ok(Some(surface)),
        None => surfaces.in_process(process),
    }
}

fn action_step(verified: bool) -> ActionStep {
    ActionStep::succeeded(CANCEL)
        .with_mechanism(StepMechanism::SemanticApi)
        .with_verified(verified)
}

fn closed_result(step: ActionStep) -> ActionResult {
    let escape = Action::PressKey(KeyCombo {
        key: "escape".to_owned(),
        modifiers: Vec::new(),
    });
    ActionResult::from_execution(&escape, vec![step])
}

/// A closed surface leaves the tree: a popover's element becomes invalid, and
/// a menu's element stays readable but is no longer a child of its parent.
pub(crate) fn surface_closed(surface: &AXElement, deadline: Deadline) -> bool {
    let gone = || match crate::tree::surface_read::element(surface, "AXParent", deadline) {
        Err(error) => error.code == ErrorCode::StaleRef,
        Ok(None) => true,
        Ok(Some(parent)) => crate::tree::surface_read::elements(&parent, "AXChildren", deadline)
            .is_ok_and(|children| {
                !children
                    .iter()
                    .any(|child| crate::tree::same_element(child, surface))
            }),
    };
    deadline.poll_until(SURFACE_CLOSE_WAIT, SURFACE_CLOSE_STEP, gone, |gone| *gone)
}

#[cfg(test)]
#[path = "escape_popup_tests.rs"]
mod tests;
