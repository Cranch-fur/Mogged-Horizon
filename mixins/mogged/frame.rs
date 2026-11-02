// Once a frame: the sweep's texture.
fn mogged_frame(game: &mut crate::game::Game<'_>) {
    crate::game::mogged_glue::frame(game);
}
