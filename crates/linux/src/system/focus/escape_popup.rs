//! The open popup of a window that does not own the active window.
//!
//! The key route sends Escape only to the active window, and core asks the
//! adapter to close the open transient surface when that key was refused for
//! focus. A popup that the application opened stays open in the background on
//! some toolkits. The popup has an owner: a combo box, or a button that has a
//! popup. The owner's own action toggles the popup, so the action is sent
//! only when a read shows the popup open, and the result is verified by
//! reading it gone. The owner's reply is the answer, so a rejected action
//! delivers nothing and an owner that does not answer leaves the delivery
//! uncertain.

use std::time::Duration;

use agent_desktop_core::{
    Action, ActionResult, AdapterError, ChainRung, Deadline, DeliveryOutcome, InteractionLease,
    InteractionPolicy, KeyCombo, ProcessIdentity, StepMechanism, WindowInfo,
};

use crate::actions::action_call::do_action;
use crate::actions::chain::run_semantic_chain;
use crate::actions::context::Target;
use crate::actions::write::mutation::WriteReply;
use crate::system::SharedBus;
use crate::system::window::window_id;
use crate::system::window::window_ops::windows_of_process;
use crate::tree::capabilities::action_names::click_verbs;
use crate::tree::menu::observe_menu::{rooted, scan_walk_structure};
use crate::tree::vocabulary::role_numbers::{
    COMBO_BOX, MENU, POPUP_MENU, PUSH_BUTTON, PUSH_BUTTON_MENU, TOGGLE_BUTTON,
};
use crate::tree::vocabulary::state_bits::{EXPANDED, HAS_POPUP, SHOWING, StateBits};
use crate::tree::{AtspiElement, ObservedRead, WalkedNode};

const SCAN_NODES: usize = 600;
const OWNER_NODES: usize = 64;
const POPUP_CONTAINER_CHILDREN: usize = 16;
const CLOSE_SETTLE: Duration = Duration::from_millis(1000);
const CLOSE_STEP: Duration = Duration::from_millis(50);
const SUGGESTION: &str = "Refresh the snapshot and check whether the popup is still open.";
const CLOSE_LABEL: &str = "Action.DoAction";

fn bits(node: &WalkedNode<ObservedRead>) -> Option<StateBits<'_>> {
    node.payload
        .node
        .traits
        .states
        .present()
        .map(|words| StateBits::new(words))
}

fn role(node: &WalkedNode<ObservedRead>) -> Option<u32> {
    node.payload.node.traits.role.present().copied()
}

fn showing_popup(node: &WalkedNode<ObservedRead>) -> bool {
    matches!(role(node), Some(MENU | POPUP_MENU))
        && bits(node).is_some_and(|bits| bits.has(SHOWING))
}

/// Whether the node owns a popup and the popup is open now. The owner roles
/// are the ones whose expansion is a popup: a combo box, and a button that
/// has the popup state. An owner is open when it reports `EXPANDED` or when a
/// popup menu below it shows. A tree row, an expander or a table cell is
/// never an owner: Escape does not collapse a tree.
pub(super) fn is_open_owner(node: &WalkedNode<ObservedRead>) -> bool {
    let Some(bits) = bits(node) else {
        return false;
    };
    let owner = match role(node) {
        Some(COMBO_BOX) => true,
        Some(PUSH_BUTTON | PUSH_BUTTON_MENU | TOGGLE_BUTTON) => bits.has(HAS_POPUP),
        _ => false,
    };
    owner && (bits.has(EXPANDED) || node.children.iter().any(showing_popup))
}

fn deepest_open_owner<'a>(
    node: &'a WalkedNode<ObservedRead>,
    depth: usize,
    best: &mut Option<(usize, &'a WalkedNode<ObservedRead>)>,
) {
    if is_open_owner(node) && best.is_none_or(|(deepest, _)| depth > deepest) {
        *best = Some((depth, node));
    }
    for child in &node.children {
        deepest_open_owner(child, depth + 1, best);
    }
}

fn frames_of(
    bus: &SharedBus,
    pid: u32,
    window: Option<&WindowInfo>,
    deadline: Deadline,
) -> Result<Vec<AtspiElement>, AdapterError> {
    let windows = match window {
        Some(window) => vec![window.clone()],
        None => windows_of_process(bus, pid, deadline)?.0,
    };
    let mut frames = Vec::new();
    for window in windows {
        let (name, path) = window_id::decode(&window.id)?;
        frames.push(rooted(&AtspiElement::new(name, path, pid)));
    }
    Ok(frames)
}

fn still_open(bus: &SharedBus, owner: &AtspiElement, deadline: Deadline) -> Option<bool> {
    let walk = scan_walk_structure(
        bus,
        &rooted(owner),
        OWNER_NODES,
        Some(POPUP_CONTAINER_CHILDREN),
        deadline,
    )
    .ok()?;
    Some(is_open_owner(&walk))
}

/// Presses the popup owner's own action and keeps its reply: no matching verb
/// or a reply of false is `WriteReply::Absent`, and a call that timed out after
/// it was sent keeps the uncertain delivery the write path gives it.
fn press_owner(
    bus: &SharedBus,
    owner: &AtspiElement,
    role: Option<u32>,
    deadline: Deadline,
) -> Result<WriteReply, AdapterError> {
    let handle = owner.clone().into_native_handle();
    let target = Target {
        bus,
        handle: &handle,
        element: owner,
        deadline,
    };
    let view = target.view()?;
    let names = view.action_names().unwrap_or_default();
    do_action(&target, names, click_verbs(role))
}

/// What the owner's reply settles. A reply of false delivered nothing, so the
/// popup is not read and cannot turn that reply into a delivery.
fn popup_outcome(reply: WriteReply, read_closed: impl FnOnce() -> Option<bool>) -> DeliveryOutcome {
    match reply {
        WriteReply::Absent => DeliveryOutcome::NotDelivered,
        WriteReply::Done => DeliveryOutcome::observed(read_closed()),
    }
}

/// The open popup of the target window, closed through its owner. `None`
/// when no popup is open, which leaves core's refusal as it was; every other
/// result, the attempt's own failure included, is the answer.
pub(crate) fn close_transient_surface(
    bus: &SharedBus,
    process: &ProcessIdentity,
    window: Option<&WindowInfo>,
    _policy: InteractionPolicy,
    lease: &InteractionLease,
) -> Result<Option<ActionResult>, AdapterError> {
    let (pid, deadline) = (process.pid.get(), lease.deadline());
    let mut best: Option<(usize, AtspiElement, Option<u32>)> = None;
    for frame in frames_of(bus, pid, window, deadline)? {
        let walk = scan_walk_structure(
            bus,
            &frame,
            SCAN_NODES,
            Some(POPUP_CONTAINER_CHILDREN),
            deadline,
        )?;
        let mut found = None;
        deepest_open_owner(&walk, 0, &mut found);
        if let Some((depth, node)) = found
            && best.as_ref().is_none_or(|(deepest, _, _)| depth > *deepest)
        {
            best = Some((depth, node.payload.node.element.clone(), role(node)));
        }
    }
    let Some((_, owner, owner_role)) = best else {
        return Ok(None);
    };
    let run = || {
        let reply = press_owner(bus, &owner, owner_role, deadline)?;
        Ok(popup_outcome(reply, || {
            deadline.poll_until(
                CLOSE_SETTLE,
                CLOSE_STEP,
                || still_open(bus, &owner, deadline).map(|open| !open),
                |closed| *closed == Some(true),
            )
        }))
    };
    let mut rungs = [ChainRung {
        label: CLOSE_LABEL,
        effects: &[],
        mechanism: StepMechanism::SemanticApi,
        run: Box::new(run),
    }];
    let escape = Action::PressKey(KeyCombo {
        key: "escape".to_owned(),
        modifiers: Vec::new(),
    });
    run_semantic_chain(&escape, &mut rungs, deadline, SUGGESTION).map(Some)
}

#[cfg(test)]
#[path = "escape_popup_tests.rs"]
mod tests;
