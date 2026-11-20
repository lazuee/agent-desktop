use super::is_open_popup;

#[test]
fn only_an_on_screen_window_at_the_popup_menu_level_is_an_open_popup() {
    assert!(is_open_popup(Some(101), true));
    assert!(!is_open_popup(Some(101), false));
    assert!(!is_open_popup(Some(0), true));
    assert!(!is_open_popup(None, true));
}
