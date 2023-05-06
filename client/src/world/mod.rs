use bevy::prelude::*;

use crate::{game_state::GameState, player::Player};

pub mod blocks;
pub mod world_map;

pub struct WorldPlugin;
impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugin(world_map::WorldMapPlugin);

        app.insert_resource(Origin(IVec3::ZERO));
        app.add_systems(
            PostUpdate,
            update_origin.run_if(in_state(GameState::Playing)),
        );
    }
}

// TODO: This could have been made to be just f64 transforms as with the server, but I don't know
// enough about the rendering stuff to replace Transform. Instead this litters conversions all over
// the place...
//
// For entities that use a Transform an offset is needed to preserve the precision of f32s. This is
// updated to be the chunk position of the player every time the player moves between chunk
// borders.
#[derive(Resource, Deref, DerefMut, Clone, Copy)]
pub struct Origin(pub IVec3);

#[derive(Component)]
pub struct MovesWithOrigin;

fn update_origin(
    mut origin: ResMut<Origin>,
    mut positions: ParamSet<(
        Query<&Transform, (Changed<Transform>, With<Player>)>,
        // Move all object roots, no UI
        Query<&mut Transform, With<MovesWithOrigin>>,
    )>,
) {
    let for_lifetime = positions.p0();
    let player_transform = if let Ok(t) = for_lifetime.get_single() {
        t
    } else {
        return;
    };

    let distance = player_transform.translation.as_ivec3() / 16;

    let translation_change = if distance != IVec3::ZERO {
        (distance * 16).as_vec3()
    } else {
        return;
    };

    origin.0 += distance * 16;

    for mut transform in positions.p1().iter_mut() {
        transform.translation -= translation_change;
    }
}
