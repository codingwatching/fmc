use sha1::Digest;
use std::io::prelude::*;

use bevy::prelude::*;
use fmc_networking::{messages, NetworkData};

mod block_textures;
mod materials;
pub mod models;

pub use block_textures::BlockTextures;
pub use materials::Materials;

/// Assets are downloaded on connection to the server. It first waits for the server config. Then
/// checks if server_config.asset_hash is the same as the hash of any stored assets. If not it asks
/// for assets from the server. It then loads them.
#[derive(States, Default, Debug, Clone, PartialEq, Eq, Hash)]
pub enum AssetState {
    #[default]
    Inactive,
    Downloading,
    Loading,
}

// Some loading actions are separated by states to allow bevy's internal systems to sync the
// needed values.
#[derive(States, Default, Debug, Clone, PartialEq, Eq, Hash)]
enum LoadingState {
    #[default]
    Inactive,
    One,
    Two,
}

// TODO: Almost all of this is workarounds for assets having to be loaded async. Bevy 0.11 will
// come with changed to the asset system, so reevaluate then.
pub struct AssetPlugin;
impl Plugin for AssetPlugin {
    fn build(&self, app: &mut App) {
        app.add_state::<AssetState>().add_state::<LoadingState>();

        app.add_systems(
            Update,
            start_asset_loading.run_if(in_state(AssetState::Inactive)),
        )
        .add_systems(
            Update,
            handle_assets_response.run_if(in_state(AssetState::Downloading)),
        )
        .add_systems(OnEnter(AssetState::Loading), start_loading)
        .add_systems(
            OnEnter(LoadingState::One),
            (
                block_textures::load_block_textures,
                models::load_models,
                crate::ui::server::key_bindings::load_key_bindings,
            ),
        )
        .add_systems(
            Update,
            test_finished_load_state_one.run_if(in_state(LoadingState::One)),
        )
        .add_systems(
            OnEnter(LoadingState::Two),
            (
                materials::load_materials,
                apply_deferred,
                crate::world::blocks::load_blocks,
                apply_deferred,
                crate::ui::server::items::load_items,
                crate::ui::server::load_interfaces,
                apply_deferred,
                finish,
            )
                .chain(),
        );
    }
}

fn test_finished_load_state_one(
    net: Res<fmc_networking::NetworkClient>,
    models: Res<models::Models>,
    asset_server: Res<AssetServer>,
    mut loading_state: ResMut<NextState<LoadingState>>,
) {
    for model in models.iter() {
        match asset_server.get_load_state(&model.handle).unwrap() {
            bevy::asset::LoadState::Failed => {
                net.disconnect(&format!(
                    "Misconfigured resource pack: Failed to load a model, check console for error."
                ));
                loading_state.set(LoadingState::Inactive);
                return;
            }
            bevy::asset::LoadState::Loaded => {
                continue;
            }
            _ => return,
        }
    }

    loading_state.set(LoadingState::Two);
}

fn start_loading(mut loading_state: ResMut<NextState<LoadingState>>) {
    loading_state.set(LoadingState::One);
}

fn finish(
    mut asset_state: ResMut<NextState<AssetState>>,
    mut loading_state: ResMut<NextState<LoadingState>>,
) {
    asset_state.set(AssetState::Inactive);
    loading_state.set(LoadingState::Inactive);
}

// TODO: The server can crash it by sending multiple server configs. The good solution would be
// proper cleanup of state between connections, and then just listen for when serverconfig is added
// as a resource, but I can't be assed.
fn start_asset_loading(
    net: Res<fmc_networking::NetworkClient>,
    mut server_config_event: EventReader<NetworkData<messages::ServerConfig>>,
    mut asset_state: ResMut<NextState<AssetState>>,
) {
    for config in server_config_event.read() {
        if !has_assets(&config.assets_hash) {
            info!("Downloading assets from the server...");
            net.send_message(messages::AssetRequest);
            asset_state.set(AssetState::Downloading)
        } else {
            asset_state.set(AssetState::Loading)
        }
    }
}

fn handle_assets_response(
    mut asset_state: ResMut<NextState<AssetState>>,
    mut asset_events: EventReader<NetworkData<messages::AssetResponse>>,
) {
    // TODO: Does this need an explicit timeout? Don't want to let the server be able to leave the
    // client in limbo without the player being able to quit.
    // TODO: Unpacking stores tarball in extraction directory, delete it.
    for tarball in asset_events.read() {
        info!("Received assets from server...");
        // Remove old assets if they exist.
        std::fs::remove_dir_all("server_assets").ok();

        let mut archive = tar::Archive::new(std::io::Cursor::new(&tarball.file));
        archive.unpack("./server_assets").unwrap();

        // Write the hash to file to check against the next time we connect.
        let mut file = std::fs::File::create("server_assets/hash.txt").unwrap();
        file.write_all(&sha1::Sha1::digest(&tarball.file)).unwrap();
        file.flush().unwrap();

        asset_state.set(AssetState::Loading);
    }
}

fn has_assets(server_hash: &Vec<u8>) -> bool {
    let mut file = match std::fs::File::open("server_assets/hash.txt") {
        Ok(f) => f,
        Err(_) => return false,
    };
    let mut hash: Vec<u8> = Vec::new();
    file.read_to_end(&mut hash).unwrap();

    if hash != *server_hash {
        return false;
    }
    return true;
}
