// A player joined the server: they get the others' presets.
fn mogged_logged_in(player: u128) {
    crate::game::mogged_glue::logged_in(player);
}
