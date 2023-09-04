use bevy::prelude::*;
use fmc_networking::{messages, ClientNetworkEvent, NetworkClient, NetworkData};

use crate::game_state::GameState;

pub struct ClientPlugin;

impl Plugin for ClientPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(fmc_networking::ClientPlugin)
            .add_systems(Update, (handle_connection, handle_server_config));
    }
}

// TODO: Disconnect and error message should be shown to player through the ui.
fn handle_connection(
    net: Res<NetworkClient>,
    mut network_events: EventReader<ClientNetworkEvent>,
    mut game_state: ResMut<NextState<GameState>>,
) {
    for event in network_events.iter() {
        match event {
            ClientNetworkEvent::Connected => {
                net.send_message(messages::ClientIdentification {
                    name: "test".to_owned(),
                });
                info!("Connected to server");
            }
            ClientNetworkEvent::Disconnected(_message) => {
                game_state.set(GameState::MainMenu);
                info!("Disconnected from server");
            }
            ClientNetworkEvent::Error(err) => {
                game_state.set(GameState::MainMenu);
                error!("{}", err);
            }
        }
    }
}

fn handle_server_config(
    mut commands: Commands,
    mut server_config_events: EventReader<NetworkData<messages::ServerConfig>>,
) {
    for event in server_config_events.iter() {
        let server_config: messages::ServerConfig = (*event).clone();
        commands.insert_resource(server_config);
    }
}
