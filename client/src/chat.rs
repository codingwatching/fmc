use bevy::prelude::*;

use fmc_networking::{messages, NetworkData};

pub struct ChatPlugin;
impl Plugin for ChatPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, handle_messages);
    }
}

fn handle_messages(mut messages: EventReader<NetworkData<messages::ChatMessage>>) {
    for message in messages.iter() {
        info!("{}: {}", message.username, message.message);
    }
}
