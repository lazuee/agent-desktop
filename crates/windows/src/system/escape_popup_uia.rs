//! The UI Automation half of `escape_popup`: the popup search, the collapse and the two entry points.

use super::opens_a_popup;
use agent_desktop_core::{
    Action, ActionResult, AdapterError, Deadline, InteractionPolicy, KeyCombo, ProcessIdentity,
    WindowInfo,
};
use uiautomation::types::{TreeScope, UIProperty};
use uiautomation::variants::Variant;

use crate::actions::disclosure::disclosure_steps;
use crate::tree::element::UIAElement;
use crate::tree::properties::property_ids::TreeProperty;
use crate::tree::properties::read_one;

const EXPANDED: i32 = 1;

/// The deepest expanded popup owner among the elements below `roots`: a
/// document-order search, so the last match is the innermost level.
fn deepest_popup_owner(roots: &[UIAElement]) -> Option<UIAElement> {
    let client = crate::tree::automation::automation_client().ok()?;
    let expanded = client
        .create_property_condition(
            UIProperty::ExpandCollapseExpandCollapseState,
            Variant::from(EXPANDED),
            None,
        )
        .ok()?;
    let mut owner = None;
    for root in roots {
        let Ok(found) = root.0.find_all(TreeScope::Descendants, &expanded) else {
            continue;
        };
        for element in found {
            let element = UIAElement::from(element);
            if read_one(&element, TreeProperty::ControlType)
                .number()
                .is_some_and(opens_a_popup)
            {
                owner = Some(element);
            }
        }
    }
    owner
}

fn window_roots(window: &WindowInfo, deadline: Deadline) -> Vec<UIAElement> {
    crate::tree::surfaces::surface_root(
        agent_desktop_core::ObservationRoot::Window(window),
        agent_desktop_core::SnapshotSurface::Window,
        deadline,
    )
    .into_iter()
    .collect()
}

fn process_roots(process: &ProcessIdentity) -> Vec<UIAElement> {
    let Ok(client) = crate::tree::automation::automation_client() else {
        return Vec::new();
    };
    let Ok(root) = client.get_root_element() else {
        return Vec::new();
    };
    let Ok(condition) = client.create_property_condition(
        UIProperty::ProcessId,
        Variant::from(i32::try_from(u32::from(process.pid)).unwrap_or(0)),
        None,
    ) else {
        return Vec::new();
    };
    root.find_all(TreeScope::Children, &condition)
        .map(|found| found.into_iter().map(UIAElement::from).collect())
        .unwrap_or_default()
}

/// Collapses the deepest open popup of the window (or of the process), and
/// reads its state back.
pub(crate) fn close_transient_surface(
    process: &ProcessIdentity,
    window: Option<&WindowInfo>,
    policy: InteractionPolicy,
    deadline: Deadline,
) -> Result<Option<ActionResult>, AdapterError> {
    let roots = match window {
        Some(window) => window_roots(window, deadline),
        None => process_roots(process),
    };
    let Some(owner) = deepest_popup_owner(&roots) else {
        return Ok(None);
    };
    let steps = disclosure_steps(&owner, false, policy, deadline)?;
    let escape = KeyCombo {
        key: "escape".to_owned(),
        modifiers: Vec::new(),
    };
    Ok(Some(ActionResult::from_execution(
        &Action::PressKey(escape),
        steps,
    )))
}
