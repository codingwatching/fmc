use std::net::SocketAddr;

use bevy::{math::DVec3, prelude::*};
use fmc_networking::{messages, NetworkServer, ServerNetworkEvent};

use crate::{
    bevy_extensions::f64_transform::{F64GlobalTransform, F64Transform},
    database::DatabaseArc,
    players::{PlayerBundle, PlayerName, PlayerRespawnEvent, Players},
    world::{
        blocks::Blocks,
        items::Items,
        models::{Model, ModelBundle, ModelVisibility, Models},
        world_map::chunk_manager::ChunkSubscriptions,
    },
};

pub struct ServerPlugin;

impl Plugin for ServerPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugin(fmc_networking::ServerPlugin)
            .add_systems(Startup, server_setup)
            // Postupdate so that no connection is removed from the Players struct mid execution,
            // while there still are packets to be handled from the connection.
            .add_systems(PreUpdate, handle_network_events);
    }
}

fn server_setup(mut net: ResMut<NetworkServer>) {
    let socket_address: SocketAddr = "127.0.0.1:42069".parse().unwrap();

    match net.listen(socket_address) {
        Ok(_) => (),
        Err(err) => {
            error!("Failed to start listening for network connections: {}", err);
            panic!();
        }
    }

    info!("Started listening for new connections!");
}

fn handle_network_events(
    mut commands: Commands,
    net: Res<NetworkServer>,
    database: Res<DatabaseArc>,
    assets_hash: Res<crate::assets::AssetArchiveHash>,
    models: Res<Models>,
    items: Res<Items>,
    mut players: ResMut<Players>,
    mut chunk_subsciptions: ResMut<ChunkSubscriptions>,
    mut network_events: EventReader<ServerNetworkEvent>,
    mut respawn_events: EventWriter<PlayerRespawnEvent>,
    player_query: Query<&PlayerName>,
) {
    for event in network_events.iter() {
        match event {
            ServerNetworkEvent::Connected(connection_id, username) => {
                info!("{} joined the server.", username);

                net.broadcast(messages::ChatMessage {
                    username: String::from("SERVER"),
                    message: format!("{} joined the server.", username),
                });

                let config = messages::ServerConfig {
                    assets_hash: assets_hash.hash.clone(),
                    block_ids: Blocks::get().clone_ids(),
                    model_ids: models.clone_ids(),
                    item_ids: items.clone_ids(),
                };
                net.send_one(*connection_id, config);

                let mut entity_commands = commands.spawn_empty();

                let player_bundle = if let Some(saved_player) = database.load_player(username) {
                    PlayerBundle::from(saved_player)
                } else {
                    // Move new players to spawn
                    respawn_events.send(PlayerRespawnEvent(entity_commands.id()));
                    PlayerBundle::new()
                };

                net.send_one(
                    *connection_id,
                    messages::PlayerConfiguration {
                        aabb_dimensions: player_bundle.aabb.half_extents.as_vec3() * 2.0,
                        camera_position: player_bundle.camera.translation.as_vec3(),
                    },
                );

                net.send_one(
                    *connection_id,
                    messages::PlayerPosition {
                        position: player_bundle.transform.translation,
                    },
                );

                net.send_one(
                    *connection_id,
                    messages::PlayerCameraRotation {
                        rotation: player_bundle.camera.rotation.as_f32(),
                    },
                );

                let entity = entity_commands
                    .with_children(|parent| {
                        parent.spawn(ModelBundle {
                            model: Model::new(models.get_id("player")),
                            visibility: ModelVisibility::new(true),
                            global_transform: F64GlobalTransform::default(),
                            transform: F64Transform {
                                //translation: player_bundle.camera.translation - player_bundle.camera.translation.y,
                                translation: DVec3::Z * 0.3 + DVec3::X * 0.3,
                                rotation: player_bundle.camera.rotation,
                                ..default()
                            },
                        });
                    })
                    .insert(player_bundle)
                    .insert(*connection_id)
                    .insert(PlayerName(username.clone()))
                    .id();

                players.insert(*connection_id, entity);
            }
            ServerNetworkEvent::Disconnected(conn_id) => {
                let entity = players.remove(conn_id).unwrap();
                chunk_subsciptions.remove_subscriber(conn_id);

                let username = player_query.get(entity).unwrap();

                net.broadcast(messages::ChatMessage {
                    username: String::from("SERVER"),
                    message: format!("{} disonnected", username.as_str()),
                });

                info!(
                    "Player disconnected, {}, username: {}",
                    conn_id,
                    username.as_str()
                );

                commands.entity(entity).despawn_recursive();
            }
            _ => {}
        }
    }
}
