// Mogged's settings page.
fn mogged_page(mod_id: &str, buttons: &mut Vec<String>) {
    crate::game::mogged_glue::settings(mod_id, buttons);
}
