#![feature(hash_drain_filter)]

use bevy::{
    diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin},
    prelude::*,
    window::WindowFocused,
};

mod assets;
mod chat;
mod constants;
mod game_state;
mod launcher;
mod networking;
mod player;
mod rendering;
mod settings;
mod ui;
mod utils;
mod world;

fn main() {
    App::new()
        //.insert_resource(Msaa { samples: 4 })
        .add_plugins(
            DefaultPlugins
                .set(AssetPlugin {
                    watch_for_changes: false,
                    asset_folder: "".to_string(),
                })
                // This is for pixel graphics
                .set(ImagePlugin::default_nearest()),
        )
        //.add_plugin(LogDiagnosticsPlugin::default())
        //.add_plugin(FrameTimeDiagnosticsPlugin::default())
        .add_plugin(networking::ClientPlugin)
        .add_plugin(assets::AssetPlugin)
        .add_plugin(game_state::GameStatePlugin)
        .add_plugin(rendering::RenderingPlugin)
        .add_plugin(player::PlayerPlugin)
        .add_plugin(world::WorldPlugin)
        .add_plugin(ui::UiPlugin)
        .add_plugin(chat::ChatPlugin)
        .insert_resource(settings::Settings::new())
        .add_systems(Startup, setup)
        .add_systems(Update, fix_keys_not_released_on_focus_loss)
        .run();
}

fn setup() {
    // TODO: These are gui assets, move to UiPlugin if there aren't more things needed.
    let assets = include_bytes!(concat!(env!("OUT_DIR"), "/assets.tar.zstd"));
    let uncompressed = zstd::stream::decode_all(assets.as_slice()).unwrap();
    let mut archive = tar::Archive::new(uncompressed.as_slice());
    for entry in archive.entries().unwrap() {
        let mut file = entry.unwrap();
        let path = file.path().unwrap();
        if !path.exists() {
            match file.unpack_in(".") {
                Err(e) => panic!(
                    "Failed to extract default assets to the resource directory.\nError: {e}"
                ),
                _ => (),
            }
        }
    }
}

// https://github.com/bevyengine/bevy/issues/4049
// https://github.com/bevyengine/bevy/issues/2068
fn fix_keys_not_released_on_focus_loss(
    mut focus_events: EventReader<WindowFocused>,
    mut key_input: ResMut<Input<KeyCode>>,
) {
    for event in focus_events.iter() {
        if !event.focused {
            key_input.bypass_change_detection().release_all();
        }
    }
}
