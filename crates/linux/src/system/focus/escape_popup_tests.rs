use agent_desktop_core::DeliveryOutcome;

use super::{is_open_owner, popup_outcome};
use crate::actions::write::mutation::WriteReply;
use crate::tree::test_support::helpers::{read_of, words};
use crate::tree::vocabulary::role_numbers::{COMBO_BOX, MENU, PUSH_BUTTON, TABLE_CELL};
use crate::tree::vocabulary::state_bits::{EXPANDED, HAS_POPUP, SHOWING};
use crate::tree::{Field, ObservedRead, WalkedNode};

fn node(
    role: u32,
    states: &[u32],
    children: Vec<WalkedNode<ObservedRead>>,
) -> WalkedNode<ObservedRead> {
    let mut read = read_of(role, "n", &[]);
    read.node.traits.states = Field::Present(words(states));
    WalkedNode {
        payload: read,
        children,
        truncated: false,
        child_count: None,
        source_index: 0,
        in_web_document: false,
    }
}

#[test]
fn only_a_combo_box_or_a_popup_button_with_an_open_popup_is_an_open_owner() {
    let shown = || vec![node(MENU, &[SHOWING], Vec::new())];
    let hidden = || vec![node(MENU, &[], Vec::new())];
    assert!(is_open_owner(&node(COMBO_BOX, &[], shown())));
    assert!(!is_open_owner(&node(COMBO_BOX, &[], hidden())));
    assert!(is_open_owner(&node(
        PUSH_BUTTON,
        &[HAS_POPUP, EXPANDED],
        Vec::new()
    )));
    assert!(!is_open_owner(&node(PUSH_BUTTON, &[EXPANDED], Vec::new())));
    assert!(!is_open_owner(&node(TABLE_CELL, &[EXPANDED], shown())));
}

#[test]
fn a_rejected_owner_action_is_never_a_delivery() {
    assert_eq!(
        popup_outcome(WriteReply::Absent, || Some(true)),
        DeliveryOutcome::NotDelivered
    );
    assert_eq!(
        popup_outcome(WriteReply::Done, || Some(true)),
        DeliveryOutcome::observed(Some(true))
    );
}
