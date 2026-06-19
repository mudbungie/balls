use super::renamed_to;

#[test]
fn tracker_maps_to_bl_tracker() {
    assert_eq!(renamed_to("tracker"), Some("bl-tracker"));
}

#[test]
fn current_and_unknown_names_have_no_rename() {
    assert_eq!(renamed_to("bl-tracker"), None);
    assert_eq!(renamed_to("bl-delivery"), None);
    assert_eq!(renamed_to("anything-else"), None);
}
