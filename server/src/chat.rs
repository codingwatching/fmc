use bevy::prelude::*;
use fmc_networking::{messages, NetworkData, NetworkServer};

use crate::players::{PlayerName, Players};

pub const CHAT_FONT_SIZE: f32 = 8.0;
pub const CHAT_TEXT_COLOR: &str = "#ffffff";

pub struct ChatPlugin;
impl Plugin for ChatPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, handle_chat_messages);
    }
}

fn handle_chat_messages(
    net: Res<NetworkServer>,
    players: Res<Players>,
    player_name_query: Query<&PlayerName>,
    mut chat_message_query: EventReader<NetworkData<messages::InterfaceTextInput>>,
) {
    for chat_message in chat_message_query.read() {
        if &chat_message.interface_path != "chat/input" {
            continue;
        }
        let player_name = player_name_query
            .get(players.get(&chat_message.source))
            .unwrap();
        let mut chat_history_update = messages::InterfaceTextBoxUpdate::new("chat/history");
        chat_history_update.append_line().with_text(
            format!("[{}] {}", &player_name.0, &chat_message.text),
            CHAT_FONT_SIZE,
            CHAT_TEXT_COLOR,
        );
        net.broadcast(chat_history_update);
    }
}
