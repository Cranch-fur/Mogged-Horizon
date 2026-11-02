// The switch key went down or up.
fn mogged_key(game: &mut crate::game::Game<'_>, key: &str, pressed: bool) {
    crate::game::mogged_glue::key(game, key, pressed);
}
