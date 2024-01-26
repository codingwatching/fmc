use bevy::{
    math::{DQuat, DVec3},
    prelude::*,
};
use std::collections::HashMap;

use fmc_networking::{messages, ConnectionId, NetworkData, NetworkServer, ServerNetworkEvent};

mod actions;
mod health;
mod inventory;
mod movement;
mod player;

// TODO: Impl save/load for database in player module to not leak.
pub use player::{Player, PlayerSave};

use crate::{
    bevy_extensions::f64_transform::{F64GlobalTransform, F64Transform},
    constants::CHUNK_SIZE,
    database::Database,
    physics::{shapes::Aabb, Velocity},
    utils,
    world::{
        blocks::Blocks,
        models::{Model, ModelBundle, ModelVisibility, Models},
        world_map::{chunk::Chunk, terrain_generation::TerrainGenerator},
        WorldProperties,
    },
};

use self::player::Camera;

pub struct PlayersPlugin;
impl Plugin for PlayersPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<RespawnEvent>()
            .add_plugins(inventory::InventoryPlugin)
            .add_plugins(health::HealthPlugin)
            .add_systems(
                Update,
                (
                    respawn_new_players,
                    respawn_players,
                    add_player_model,
                    send_player_configuration,
                    handle_player_position_updates,
                    handle_player_rotation_updates,
                    actions::handle_left_clicks,
                    actions::handle_right_clicks,
                ),
            )
            .add_systems(PreUpdate, add_and_remove_players);
    }
}

fn add_and_remove_players(
    mut commands: Commands,
    database: Res<Database>,
    player_query: Query<(Option<&Player>, &ConnectionId)>,
    mut network_events: EventReader<ServerNetworkEvent>,
) {
    for event in network_events.read() {
        match event {
            ServerNetworkEvent::Connected { entity, username } => {
                let player_bundle = if let Some(player_save) = database.load_player(username) {
                    player_save.into()
                } else {
                    player::PlayerBundle::default()
                };

                commands.entity(*entity).insert((
                    Player {
                        username: username.to_owned(),
                    },
                    player_bundle,
                ));

                let (_, connection_id) = player_query.get(*entity).unwrap();
                info!(
                    "Player connected, id: {}, username: {}",
                    connection_id, username
                );
            }
            ServerNetworkEvent::Disconnected { entity } => {
                let (player, connection_id) = player_query.get(*entity).unwrap();
                info!(
                    "Player disconnected, id: {}, username: {}",
                    connection_id,
                    player.unwrap().username
                );
            }
            _ => {}
        }
    }
}

fn send_player_configuration(
    net: Res<NetworkServer>,
    player_query: Query<(&ConnectionId, &Aabb, &Camera, &F64Transform), Added<Player>>,
) {
    for (connection, aabb, camera, transform) in player_query.iter() {
        net.send_one(
            *connection,
            messages::PlayerConfiguration {
                aabb_dimensions: aabb.half_extents.as_vec3() * 2.0,
                camera_position: camera.translation.as_vec3(),
            },
        );

        net.send_one(
            *connection,
            messages::PlayerPosition {
                position: transform.translation,
                velocity: DVec3::ZERO,
            },
        );

        net.send_one(
            *connection,
            messages::PlayerCameraRotation {
                rotation: camera.rotation.as_f32(),
            },
        );
    }
}

fn add_player_model(
    mut commands: Commands,
    models: Res<Models>,
    player_query: Query<(Entity, &Camera), Added<Player>>,
) {
    for (entity, camera) in player_query.iter() {
        commands.entity(entity).with_children(|parent| {
            parent.spawn(ModelBundle {
                model: Model::new(models.get_id("player")),
                visibility: ModelVisibility::default(),
                global_transform: F64GlobalTransform::default(),
                transform: F64Transform {
                    //translation: player_bundle.camera.translation - player_bundle.camera.translation.y,
                    translation: DVec3::Z * 0.3 + DVec3::X * 0.3,
                    rotation: camera.rotation,
                    ..default()
                },
            });
        });
    }
}

fn handle_player_position_updates(
    net: Res<NetworkServer>,
    mut player_query: Query<(&mut F64Transform, &mut Velocity), With<Player>>,
    mut position_events: EventReader<NetworkData<messages::PlayerPosition>>,
) {
    for position_update in position_events.read() {
        let (mut player_position, mut player_velocity) = player_query
            .get_mut(position_update.source.entity())
            .unwrap();
        player_position.translation = position_update.position;
        player_velocity.0 = position_update.velocity;
    }
}

// Client sends the rotation of its camera. Used to know where they are looking, and
// how the player model should be positioned.
fn handle_player_rotation_updates(
    mut player_query: Query<(&mut player::Camera, &Children)>,
    mut player_model_transforms: Query<&mut F64Transform, With<Model>>,
    mut camera_rotation_events: EventReader<NetworkData<messages::PlayerCameraRotation>>,
) {
    for rotation_update in camera_rotation_events.read() {
        let (mut camera, children) = player_query
            .get_mut(rotation_update.source.entity())
            .unwrap();
        camera.rotation = rotation_update.rotation.as_f64();

        let mut transform = player_model_transforms
            .get_mut(*children.first().unwrap())
            .unwrap();
        let theta = f64::atan2(camera.rotation.y, camera.rotation.w);
        transform.rotation = DQuat::from_xyzw(0.0, f64::sin(theta), 0.0, f64::cos(theta));
    }
}

#[derive(Event)]
pub struct RespawnEvent {
    pub entity: Entity,
}

fn respawn_new_players(
    player_query: Query<(Entity, &F64Transform), Added<Player>>,
    mut respawn_events: EventWriter<RespawnEvent>,
) {
    for (entity, transform) in player_query.iter() {
        if transform.translation == DVec3::ZERO {
            respawn_events.send(RespawnEvent { entity });
        }
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
    terrain_generator: Res<TerrainGenerator>,
    database: Res<Database>,
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
                break chunk_position;
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
