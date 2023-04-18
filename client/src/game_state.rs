use bevy::prelude::*;

use crate::assets::AssetState;

/// The overarching states the game can be in.
#[derive(States, PartialEq, Eq, Debug, Clone, Hash, Default)]
pub enum GameState {
    /// Player is in the main menu.
    #[default]
    MainMenu,
    /// Client is negotiating the connection.
    Connecting,
    /// Playing on a server.
    Playing,
}

pub struct GameStatePlugin;
impl Plugin for GameStatePlugin {
    fn build(&self, app: &mut App) {
        app.add_state::<GameState>();
        app.add_systems(OnExit(AssetState::Loading), finished_loading_start_game);
    }
}

// All assets are loaded, it can now start the main game loop
fn finished_loading_start_game(mut state: ResMut<NextState<GameState>>) {
    state.set(GameState::Playing);
}
