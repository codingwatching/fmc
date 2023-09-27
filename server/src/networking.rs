use std::net::SocketAddr;

use bevy::prelude::*;
use fmc_networking::{messages, NetworkData, NetworkServer, ServerNetworkEvent};

use crate::{
    database::DatabaseArc,
    players::{PlayerName, Players},
    settings::ServerSettings,
    world::{blocks::Blocks, items::Items, models::Models}, chat::{CHAT_FONT_SIZE, CHAT_TEXT_COLOR},
};

pub struct ServerPlugin;

impl Plugin for ServerPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(fmc_networking::ServerPlugin)
            .add_systems(PostStartup, server_setup)
            // Postupdate to ensure all packets from disconnected clients have been handled before
            // the connection is removed.
            .add_systems(PostUpdate, handle_network_events);
    }
}

fn server_setup(
    mut commands: Commands,
    mut net: ResMut<NetworkServer>,
    assets_hash: Res<crate::assets::AssetArchiveHash>,
    models: Res<Models>,
    items: Res<Items>,
    server_settings: Res<ServerSettings>,
) {
    let socket_address: SocketAddr = "127.0.0.1:42069".parse().unwrap();

    net.listen(socket_address);

    commands.insert_resource(messages::ServerConfig {
        assets_hash: assets_hash.hash.clone(),
        block_ids: Blocks::get().clone_ids(),
        model_ids: models.clone_ids(),
        item_ids: items.clone_ids(),
        render_distance: server_settings.render_distance,
    });

    info!("Started listening for new connections!");
}

fn handle_network_events(
    mut commands: Commands,
    net: Res<NetworkServer>,
    server_config: Res<messages::ServerConfig>,
    mut players: ResMut<Players>,
    mut network_events: EventReader<ServerNetworkEvent>,
    player_query: Query<&PlayerName>,
) {
    for event in network_events.read() {
        match event {
            ServerNetworkEvent::Connected {
                connection,
                username,
            } => {
                net.send_one(*connection, server_config.clone());

                // The PlayerBundle is inserted on Added<PlayerName>(next tick), but is
                // guaranteed to be available by the time the first packet is processed.
                let entity = commands
                    .spawn((*connection, PlayerName(username.clone())))
                    .id();
                players.insert(*connection, entity);

                info!("{} joined the server.", username);

                let mut chat_update = messages::InterfaceTextBoxUpdate::new("chat/history");
                chat_update.append_line().with_text(format!("[SERVER] {} joined the server.", username), CHAT_FONT_SIZE, CHAT_TEXT_COLOR);
                net.broadcast(chat_update);
            }
            ServerNetworkEvent::Disconnected(connection) => {
                let entity = players.remove(connection).unwrap();
                let username = player_query.get(entity).unwrap();

                let mut chat_update = messages::InterfaceTextBoxUpdate::new("chat/history");
                chat_update.append_line().with_text(format!("[SERVER] {} disconnected", username.as_str()), CHAT_FONT_SIZE, CHAT_TEXT_COLOR);
                net.broadcast(chat_update);

                info!(
                    "Player disconnected, id: {}, username: {}",
                    connection,
                    username.as_str()
                );

                commands.entity(entity).despawn_recursive();
            }
            _ => {}
        }
    }
}
