// A player's face in the player list or spectator menu: the preset's while one is on.
fn mogged_face(value: String, game: &mut crate::game::Game<'_>, uuid: u128) -> String {
    let own = game.remote.as_ref().map(|r| r.profile);
    crate::game::mogged_glue::drawn_skin(game, own, uuid, (value, false)).0
}
