use super::imp::press_delivered;
use agent_desktop_core::WitnessChange;

#[test]
fn a_press_is_delivered_by_a_change_in_the_target_or_its_focus_but_not_by_the_window_alone() {
    for change in [
        WitnessChange::TargetSelected,
        WitnessChange::TargetValue,
        WitnessChange::SurfaceOpened,
        WitnessChange::FocusIntoTarget,
        WitnessChange::FocusMovedInWindow,
        WitnessChange::FocusedValue,
    ] {
        assert!(press_delivered(&[change]), "{change:?}");
    }
    for change in [
        WitnessChange::WindowTitle,
        WitnessChange::WindowContent,
        WitnessChange::WindowCount,
        WitnessChange::TargetGone,
    ] {
        assert!(!press_delivered(&[change]), "{change:?}");
    }
    assert!(!press_delivered(&[]));
}
