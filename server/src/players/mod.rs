use bevy::{
    math::{DQuat, DVec3},
    prelude::*,
};
use std::collections::HashMap;

use fmc_networking::{messages, ConnectionId, NetworkData, NetworkServer};

mod actions;
mod inventory;
mod player;

pub use player::*;

use crate::{
    bevy_extensions::f64_transform::{F64GlobalTransform, F64Transform},
    world::{models::Model, world_map::chunk_manager::ChunkSubscriptions},
};

pub struct PlayersPlugin;
impl Plugin for PlayersPlugin {
    fn build(&self, app: &mut App) {
        app //.add_event::<PlayerDeathEvent>()
            .add_event::<PlayerRespawnEvent>()
            .insert_resource(Players::default())
            .add_plugin(inventory::InventoryPlugin)
            .add_systems(
                Update,
                (
                    respawn_players,
                    handle_player_position_updates,
                    handle_player_rotation_updates,
                    actions::handle_left_clicks,
                    actions::place_blocks,
                ),
            );
    }
}

pub struct PlayerRespawnEvent(pub Entity);

//pub struct PlayerDeathEvent(Entity);

#[derive(Deref, DerefMut, Resource)]
pub struct Players(HashMap<ConnectionId, Entity>);

impl Players {
    #[track_caller]
    pub fn get(&self, conn_id: &ConnectionId) -> Entity {
        return match self.0.get(conn_id) {
            Some(e) => *e,
            None => panic!(
                "Could not find a player entity for the connection {}",
                conn_id
            ),
        };
    }
}

// Can't derive default for some reason
impl Default for Players {
    fn default() -> Self {
        Self(HashMap::new())
    }
}

fn handle_player_position_updates(
    net: Res<NetworkServer>,
    players: Res<Players>,
    chunk_subscriptions: Res<ChunkSubscriptions>,
    mut player_query: Query<&mut F64Transform, With<PlayerMarker>>,
    mut position_events: EventReader<NetworkData<messages::PlayerPosition>>,
) {
    for position_update in position_events.iter() {
        let player_entity = players.get(&position_update.source);
        let mut player_position = player_query.get_mut(player_entity).unwrap();
        player_position.translation = position_update.position;
        //let response = messages::UpdateModelPosition {
        //    id: models.head.id
        //    position: transform.translation,
        //};
    }
}

// Client sends the rotation of its camera. Used to know where they are looking, and
// how the player model should be positioned.
fn handle_player_rotation_updates(
    net: Res<NetworkServer>,
    players: Res<Players>,
    chunk_subscriptions: Res<ChunkSubscriptions>,
    mut player_query: Query<(&mut PlayerCamera, &Children)>,
    mut player_model_transforms: Query<&mut F64Transform, With<Model>>,
    mut camera_rotation_events: EventReader<NetworkData<messages::PlayerCameraRotation>>,
) {
    for rotation_update in camera_rotation_events.iter() {
        let entity = players.get(&rotation_update.source);
        let (mut camera, children) = player_query.get_mut(entity).unwrap();
        camera.rotation = rotation_update.rotation.as_f64();

        let mut transform = player_model_transforms
            .get_mut(*children.first().unwrap())
            .unwrap();
        let theta = f64::atan2(camera.rotation.y, camera.rotation.w);
        transform.rotation = DQuat::from_xyzw(0.0, f64::sin(theta), 0.0, f64::cos(theta));
    }
}

// TODO: Actually respawn
fn respawn_players(
    net: Res<NetworkServer>,
    mut respawn_events: EventReader<PlayerRespawnEvent>,
    connection_query: Query<&ConnectionId>,
) {
    for event in respawn_events.iter() {
        if let Ok(connection_id) = connection_query.get(event.0) {
            net.send_one(
                *connection_id,
                messages::PlayerPosition {
                    position: DVec3::new(0.0, 240.0, 0.0),
                },
            );
        }
    }
}
