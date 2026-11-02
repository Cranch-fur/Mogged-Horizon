// Before the inventory (and its preview of the player) is drawn: the preset skin on the own model.
fn mogged_inventory(game: &mut crate::game::Game<'_>) {
    crate::game::remote::mogged_remote::apply(game);
}
