use bevy::prelude::*;
use fmc_networking::{messages, NetworkClient};

use crate::assets::AssetState;

/// The overarching states the game can be in.
#[derive(States, PartialEq, Eq, Debug, Clone, Hash, Default)]
pub enum GameState {
    #[default]
    MainMenu,
    Connecting,
    Playing,
    Paused,
}

pub struct GameStatePlugin;
impl Plugin for GameStatePlugin {
    fn build(&self, app: &mut App) {
        app.add_state::<GameState>();
        app.add_systems(OnExit(AssetState::Loading), finished_loading_start_game);
    }
}

// All assets are loaded, it can now start the main game loop
fn finished_loading_start_game(net: Res<NetworkClient>, mut state: ResMut<NextState<GameState>>) {
    net.send_message(messages::ClientFinishedLoading);
    state.set(GameState::Playing);
}
