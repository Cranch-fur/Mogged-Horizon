// Once a tick: the window sends its presets, the server relays them.
fn mogged_tick(game: &mut crate::game::Game<'_>) {
    crate::game::mogged_glue::tick(game);
}
