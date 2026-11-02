// The preset skins on the models Horizon draws. Added to `game/remote.rs` as
// `game::remote::mogged_remote`, where the connection's player list is visible.
use crate::game::Game;

/// Horizon gives every player model its own skin again each frame, in the HUD
/// (`update_remote_skins`), before it builds the models: puts the preset's on
/// the level's players and the own model while one is on. Called before the
/// world's models and before the inventory's preview are drawn.
pub(crate) fn apply(game: &mut Game<'_>) {
    let Some(r) = game.remote.as_ref() else {
        return;
    };
    let own = r.profile;
    let mut level: Vec<(u32, u128)> = Vec::new();
    for (id, uuid) in r.level_players.iter() {
        level.push((*id as u32, *uuid));
    }
    for (id, uuid) in level {
        let mut found = None;
        for (i, mob) in game.mobs.iter().enumerate() {
            if mob.id == id && &*mob.kind.id == "minecraft:player" {
                found = Some(i);
                break;
            }
        }
        let Some(i) = found else {
            continue;
        };
        let Some(skin) = game.mobs[i].skin.clone() else {
            continue;
        };
        let slim = game.mobs[i].slim;
        let (skin, slim) = crate::game::mogged_glue::drawn_skin(game, Some(own), uuid, (skin, slim));
        game.mobs[i].skin = Some(skin);
        game.mobs[i].slim = slim;
    }
    let mut model: Option<(String, bool)> = None;
    if let Some(mob) = game.player_model.as_ref() {
        if let Some(skin) = &mob.skin {
            model = Some((skin.clone(), mob.slim));
        }
    }
    if let Some(drawn) = model {
        let (skin, slim) = crate::game::mogged_glue::drawn_skin(game, Some(own), own, drawn);
        if let Some(mob) = game.player_model.as_mut() {
            mob.skin = Some(skin);
            mob.slim = slim;
        }
    }
}
