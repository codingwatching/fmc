use bevy::prelude::*;
use fmc_networking::{messages, NetworkData, NetworkServer, ServerNetworkEvent};

use crate::players::Player;

pub const CHAT_FONT_SIZE: f32 = 8.0;
pub const CHAT_TEXT_COLOR: &str = "#ffffff";

pub struct ChatPlugin;
impl Plugin for ChatPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (handle_chat_messages, send_connection_messages));
    }
}

fn handle_chat_messages(
    net: Res<NetworkServer>,
    player_query: Query<&Player>,
    mut chat_message_query: EventReader<NetworkData<messages::InterfaceTextInput>>,
) {
    for chat_message in chat_message_query.read() {
        if &chat_message.interface_path != "chat/input" {
            continue;
        }
        let player = player_query.get(chat_message.source.entity()).unwrap();
        let mut chat_history_update = messages::InterfaceTextBoxUpdate::new("chat/history");
        chat_history_update.append_line().with_text(
            format!("[{}] {}", &player.username, &chat_message.text),
            CHAT_FONT_SIZE,
            CHAT_TEXT_COLOR,
        );
        net.broadcast(chat_history_update);
    }
}

fn send_connection_messages(
    net: Res<NetworkServer>,
    player_query: Query<&Player>,
    mut network_events: EventReader<ServerNetworkEvent>,
) {
    for event in network_events.read() {
        match event {
            ServerNetworkEvent::Connected { username, .. } => {
                let mut chat_update = messages::InterfaceTextBoxUpdate::new("chat/history");
                chat_update.append_line().with_text(
                    format!("{} joined the game", username),
                    CHAT_FONT_SIZE,
                    CHAT_TEXT_COLOR,
                );
                net.broadcast(chat_update);
            }
            ServerNetworkEvent::Disconnected { entity } => {
                let player = player_query.get(*entity).unwrap();
                let mut chat_update = messages::InterfaceTextBoxUpdate::new("chat/history");
                chat_update.append_line().with_text(
                    format!("{} left the game", player.username),
                    CHAT_FONT_SIZE,
                    CHAT_TEXT_COLOR,
                );
                net.broadcast(chat_update);
            }
            _ => (),
        }
    }
}
