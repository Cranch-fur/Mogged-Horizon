// Before the world's models are built: the preset skins on them.
fn mogged_models(game: &mut crate::game::Game<'_>) {
    crate::game::remote::mogged_remote::apply(game);
}
