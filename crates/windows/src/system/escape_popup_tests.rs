use super::opens_a_popup;

#[test]
fn a_tree_item_or_a_group_is_never_a_popup_owner() {
    assert!(opens_a_popup(50011) && opens_a_popup(50003) && opens_a_popup(50031));
    assert!(!opens_a_popup(50024) && !opens_a_popup(50026) && !opens_a_popup(50029));
}
