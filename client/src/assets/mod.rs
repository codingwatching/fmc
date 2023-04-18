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
    Downloading,
    Loading,
    #[default]
    Waiting,
}

pub struct AssetPlugin;
impl Plugin for AssetPlugin {
    fn build(&self, app: &mut App) {
        app.add_state::<AssetState>();

        app.add_systems(
            Update,
            start_asset_loading.run_if(in_state(AssetState::Waiting)),
        )
        .add_systems(
            Update,
            handle_assets_response.run_if(in_state(AssetState::Downloading)),
        )
        // TODO: Everything between apply_system_buffers can happen in parallel, but I don't
        // know how to do it, I don't grasp the new scheduling system yet.
        .add_systems(
            OnEnter(AssetState::Loading),
            (
                block_textures::load_block_textures,
                models::load_models,
                crate::player::key_bindings::load_key_bindings,
                apply_system_buffers,
                crate::player::interfaces::load_items.after(models::load_models),
                crate::player::interfaces::load_interfaces,
                materials::load_materials.after(block_textures::load_block_textures),
                apply_system_buffers,
                crate::world::blocks::load_blocks.after(materials::load_materials),
                finish,
            )
                .chain(),
        );
    }
}

fn finish(mut asset_state: ResMut<NextState<AssetState>>) {
    asset_state.set(AssetState::Waiting);
}

// TODO: The server can crash it by sending multiple server configs. The good solution would be
// proper cleanup of state between connections, and then just listen for when serverconfig is added
// as a resource, but I can't be assed.
fn start_asset_loading(
    net: Res<fmc_networking::NetworkClient>,
    mut server_config_event: EventReader<NetworkData<messages::ServerConfig>>,
    mut asset_state: ResMut<NextState<AssetState>>,
) {
    for config in server_config_event.iter() {
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
    for tarball in asset_events.iter() {
        info!("Received assets from server...");
        // Remove old assets if they exist.
        match std::fs::remove_dir_all("server_assets") {
            Ok(_) => (),
            Err(e) => warn!(
                "Failed to delete old assets folder while downloading new assets!\nError: {}",
                e
            ),
        };

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
