// A click on Mogged's settings page.
fn mogged_click(mod_id: &str, button: usize) {
    crate::game::mogged_glue::click(mod_id, button);
}
