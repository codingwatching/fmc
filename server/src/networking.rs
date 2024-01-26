use std::net::SocketAddr;

use bevy::prelude::*;
use fmc_networking::{messages, ConnectionId, NetworkServer, ServerNetworkEvent};

use crate::{
    settings::Settings,
    world::{blocks::Blocks, items::Items, models::Models},
};

// TODO: I stripped this for most of its functionality, and it's a little too lean now. Move server
// setup to main, and sending the server config to fmc_networking::server
pub struct ServerPlugin;
impl Plugin for ServerPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(fmc_networking::ServerPlugin)
            .add_systems(PostStartup, server_setup)
            .add_systems(Update, handle_network_events);
    }
}

fn server_setup(
    mut commands: Commands,
    mut net: ResMut<NetworkServer>,
    assets_hash: Res<crate::assets::AssetArchiveHash>,
    models: Res<Models>,
    items: Res<Items>,
    settings: Res<Settings>,
) {
    let socket_address: SocketAddr = "127.0.0.1:42069".parse().unwrap();

    net.listen(socket_address);

    commands.insert_resource(messages::ServerConfig {
        assets_hash: assets_hash.hash.clone(),
        block_ids: Blocks::get().clone_ids(),
        model_ids: models.clone_ids(),
        item_ids: items.clone_ids(),
        render_distance: settings.render_distance,
    });

    info!("Started listening for new connections!");
}

fn handle_network_events(
    net: Res<NetworkServer>,
    server_config: Res<messages::ServerConfig>,
    connection_query: Query<&ConnectionId>,
    mut network_events: EventReader<ServerNetworkEvent>,
) {
    for event in network_events.read() {
        match event {
            ServerNetworkEvent::Connected { entity, .. } => {
                let connection_id = connection_query.get(*entity).unwrap();
                net.send_one(*connection_id, server_config.clone());
            }
            _ => {}
        }
    }
}
