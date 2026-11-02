// A player left the server: the others forget their presets.
fn mogged_logged_out(player: u128) {
    crate::game::mogged_glue::logged_out(player);
}
