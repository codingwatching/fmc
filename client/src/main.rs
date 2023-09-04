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
        .insert_resource(FixedTime::new_from_secs(1.0 / 144.0))
        .add_plugins(
            DefaultPlugins
                .set(AssetPlugin {
                    watch_for_changes: None,
                    asset_folder: "".to_string(),
                })
                .set(ImagePlugin::default_nearest()),
        )
        //.add_plugin(LogDiagnosticsPlugin::default())
        //.add_plugin(FrameTimeDiagnosticsPlugin::default())
        .add_plugins(networking::ClientPlugin)
        .add_plugins(assets::AssetPlugin)
        .add_plugins(game_state::GameStatePlugin)
        .add_plugins(rendering::RenderingPlugin)
        .add_plugins(player::PlayerPlugin)
        .add_plugins(world::WorldPlugin)
        .add_plugins(ui::UiPlugin)
        .add_plugins(chat::ChatPlugin)
        .add_plugins(settings::SettingsPlugin)
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
