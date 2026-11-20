//! The readback for `perform`, which has no written attribute to re-read. A
//! press is delivered when the effect witness of its target changes the way a
//! press changes it: the target's selection turns on or its value changes, a
//! menu or popover opens below it, or focus moves into it or within its
//! window, or the focused element in its window shows new content. Focus
//! alone misses a whole family of controls (a sidebar row moves its selection,
//! a radio button turns its value on), so all of these are read. The window's
//! title and content are not read here: they can change by themselves.

#[cfg(target_os = "macos")]
mod imp {
    use agent_desktop_core::{Deadline, EffectWitness, WitnessChange};

    use crate::actions::effect_witness;
    use crate::tree::AXElement;

    const SETTLE_POLL_MS: u64 = 40;
    const SETTLE_BUDGET_MS: u64 = 400;

    pub(crate) fn focus_state(element: &AXElement, deadline: Deadline) -> Option<EffectWitness> {
        effect_witness::read_local(element, deadline).ok()
    }

    pub(crate) fn changed_now(
        before: &Option<EffectWitness>,
        element: &AXElement,
        deadline: Deadline,
    ) -> bool {
        match (before, focus_state(element, deadline)) {
            (Some(before), Some(after)) => press_delivered(&after.changes_since(before)),
            _ => false,
        }
    }

    pub(crate) fn settled_change(
        before: &Option<EffectWitness>,
        element: &AXElement,
        deadline: Deadline,
    ) -> bool {
        if before.is_none() {
            return false;
        }
        let deadline = deadline.capped(std::time::Duration::from_millis(SETTLE_BUDGET_MS));
        while !deadline.is_expired() {
            if changed_now(before, element, deadline) {
                return true;
            }
            std::thread::sleep(
                deadline
                    .remaining()
                    .min(std::time::Duration::from_millis(SETTLE_POLL_MS)),
            );
        }
        false
    }

    /// The changes that show a press reached the application.
    pub(crate) fn press_delivered(changes: &[WitnessChange]) -> bool {
        changes.iter().any(|change| {
            matches!(
                change,
                WitnessChange::TargetSelected
                    | WitnessChange::TargetValue
                    | WitnessChange::SurfaceOpened
                    | WitnessChange::FocusIntoTarget
                    | WitnessChange::FocusMovedInWindow
                    | WitnessChange::FocusedValue
            )
        })
    }
}

pub(crate) use imp::{changed_now, focus_state, settled_change};

#[cfg(test)]
#[path = "activation_effect_tests.rs"]
mod tests;
