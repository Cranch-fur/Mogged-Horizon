// Other players' presets, from the server.
fn mogged_from_server(game: &mut crate::game::Game<'_>, channel: &str, data: &[u8]) {
    crate::game::mogged_glue::from_server(game, channel, data);
}
