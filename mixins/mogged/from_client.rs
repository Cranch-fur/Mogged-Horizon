// A player's presets, for the server to pass on.
fn mogged_from_client(player: u128, channel: &str, data: &[u8]) {
    crate::game::mogged_glue::from_client(player, channel, data);
}
