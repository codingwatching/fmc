use bevy::{
    math::{DQuat, DVec3},
    prelude::*,
};
use std::collections::HashMap;

use fmc_networking::{messages, ConnectionId, NetworkData, NetworkServer};

mod actions;
mod life;
mod inventory;
mod movement;
mod player;

pub use player::{PlayerMarker, PlayerName, PlayerSave};

use crate::{
    bevy_extensions::f64_transform::{F64GlobalTransform, F64Transform},
    constants::CHUNK_SIZE,
    database::DatabaseArc,
    physics::Velocity,
    utils,
    world::{
        blocks::Blocks,
        models::{Model, ModelBundle, ModelVisibility, Models},
        world_map::{chunk::Chunk, terrain_generation::TerrainGeneratorArc},
        WorldProperties,
    },
};

pub struct PlayersPlugin;
impl Plugin for PlayersPlugin {
    fn build(&self, app: &mut App) {
        app .add_event::<RespawnEvent>()
            .insert_resource(Players::default())
            .add_plugins(inventory::InventoryPlugin)
            .add_plugins(life::LifePlugin)
            // This has to be preupdate to ensure that all player components are available by the
            // time the first packet handlers might access them.
            .add_systems(PreUpdate, add_players)
            .add_systems(
                Update,
                (
                    respawn_players,
                    handle_player_position_updates,
                    handle_player_rotation_updates,
                    actions::handle_left_clicks,
                    actions::handle_right_clicks,
                ),
            );
    }
}

#[derive(Event)]
pub struct RespawnEvent{
    pub entity: Entity
}

#[derive(Default, Deref, DerefMut, Resource)]
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

fn add_players(
    mut commands: Commands,
    net: Res<NetworkServer>,
    database: Res<DatabaseArc>,
    models: Res<Models>,
    mut respawn_events: EventWriter<RespawnEvent>,
    player_query: Query<(Entity, &ConnectionId, &PlayerName), Added<PlayerName>>,
) {
    for (player_entity, connection, username) in player_query.iter() {
        let player_bundle = if let Some(saved_player) = database.load_player(username) {
            player::PlayerBundle::from(saved_player)
        } else {
            respawn_events.send(RespawnEvent{
                entity: player_entity
            });
            player::PlayerBundle::default()
        };

        net.send_one(
            *connection,
            messages::PlayerConfiguration {
                aabb_dimensions: player_bundle.aabb.half_extents.as_vec3() * 2.0,
                camera_position: player_bundle.camera.translation.as_vec3(),
            },
        );

        net.send_one(
            *connection,
            messages::PlayerPosition {
                position: player_bundle.transform.translation,
                velocity: DVec3::ZERO,
            },
        );

        net.send_one(
            *connection,
            messages::PlayerCameraRotation {
                rotation: player_bundle.camera.rotation.as_f32(),
            },
        );

        commands
            .entity(player_entity)
            .with_children(|parent| {
                parent.spawn(ModelBundle {
                    model: Model::new(models.get_id("player")),
                    visibility: ModelVisibility::default(),
                    global_transform: F64GlobalTransform::default(),
                    transform: F64Transform {
                        //translation: player_bundle.camera.translation - player_bundle.camera.translation.y,
                        translation: DVec3::Z * 0.3 + DVec3::X * 0.3,
                        rotation: player_bundle.camera.rotation,
                        ..default()
                    },
                });
            })
            .insert(player_bundle);
    }
}

fn handle_player_position_updates(
    net: Res<NetworkServer>,
    players: Res<Players>,
    mut player_query: Query<(&mut F64Transform, &mut Velocity), With<PlayerMarker>>,
    mut position_events: EventReader<NetworkData<messages::PlayerPosition>>,
) {
    for position_update in position_events.read() {
        let player_entity = players.get(&position_update.source);
        let (mut player_position, mut player_velocity) =
            player_query.get_mut(player_entity).unwrap();
        player_position.translation = position_update.position;
        player_velocity.0 = position_update.velocity;
    }
}

// Client sends the rotation of its camera. Used to know where they are looking, and
// how the player model should be positioned.
fn handle_player_rotation_updates(
    players: Res<Players>,
    mut player_query: Query<(&mut player::Camera, &Children)>,
    mut player_model_transforms: Query<&mut F64Transform, With<Model>>,
    mut camera_rotation_events: EventReader<NetworkData<messages::PlayerCameraRotation>>,
) {
    for rotation_update in camera_rotation_events.read() {
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

// TODO: If it can't find a valid spawn point it will just oscillate in an infinite loop between the
// air chunk above and the one it can't find anything in.
// TODO: This might take a really long time to compute because of the chunk loading, and should
// probably be done ahead of time through an async task. Idk if the spawn point should change
// between each spawn. A good idea if it's really hard to validate that the player won't suffocate
// infinitely.
fn respawn_players(
    net: Res<NetworkServer>,
    world_properties: Res<WorldProperties>,
    terrain_generator: Res<TerrainGeneratorArc>,
    database: Res<DatabaseArc>,
    mut respawn_events: EventReader<RespawnEvent>,
    connection_query: Query<&ConnectionId>,
) {
    for event in respawn_events.read() {
        let blocks = Blocks::get();
        let air = blocks.get_id("air");

        let mut chunk_position =
            utils::world_position_to_chunk_position(world_properties.spawn_point.center);
        let spawn_position = 'outer: loop {
            let chunk = futures_lite::future::block_on(Chunk::load(
                chunk_position,
                terrain_generator.clone(),
                database.clone(),
            ))
            .1;

            if chunk.is_uniform() && chunk[0] == air {
                chunk_position.y -= CHUNK_SIZE as i32;
                continue;
            }

            // Find a spot that has a block with two air blocks above.
            for (i, block_chunk) in chunk.blocks.chunks_exact(CHUNK_SIZE).enumerate() {
                let mut count = 0;
                for (j, block) in block_chunk.iter().enumerate() {
                    //if count == 3 {
                    if count == 2 {
                        let mut spawn_position =
                            chunk_position + utils::block_index_to_position(i * CHUNK_SIZE + j);
                        spawn_position.y -= 2;
                        break 'outer spawn_position;
                    //} else if count == 0 && *block != air {
                    } else if count == 3 && *block != air {
                        count += 1;
                        //match blocks.get_config(&block).friction {
                        //    Friction::Drag(_) => continue,
                        //    _ => count += 1,
                        //};
                        //} else if count == 1 && *block == air {
                    } else if count == 0 && *block == air {
                        count += 1;
                    //} else if count == 2 && *block == air {
                    } else if count == 1 && *block == air {
                        count += 1;
                    } else {
                        count = 0;
                    }
                }
            }

            chunk_position.y += CHUNK_SIZE as i32;
        };

        let connection_id = connection_query.get(event.entity).unwrap();
        net.send_one(
            *connection_id,
            messages::PlayerPosition {
                position: spawn_position.as_dvec3(),
                velocity: DVec3::ZERO,
            },
        );
    }
}
