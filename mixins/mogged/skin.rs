// The skin Horizon draws for a player (its model, its face in the player
// list): the preset's while one is on.
fn mogged_skin(game: &mut crate::game::Game<'_>, uuid: u128, value: (String, bool)) -> (String, bool) {
    let own = game.remote.as_ref().map(|r| r.profile);
    crate::game::mogged_glue::drawn_skin(game, own, uuid, value)
}
