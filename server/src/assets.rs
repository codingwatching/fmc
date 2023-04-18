use bevy::prelude::*;
use fmc_networking::{messages, NetworkData, NetworkServer};
use sha1::Digest;

pub struct AssetPlugin;
impl Plugin for AssetPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, make_asset_tarball)
            // Run this in PreStartup so ObjectIds are available for other startup systems.
            .add_systems(Update, handle_asset_requests);

        // The server's necessary assets are included at compile time, two birds one stone.
        // 1. The assets are always available without having to fetch them from the web.
        // TODO: Mods ruin this
        // 2. We do not need to have a list of necessary blocks/items/models included in the
        //    source. Although if compiled without the required assets, it will cause unexpected panics.
        // Every time a new world file is initialized the assets are unpacked without overwriting.
        // The server can then read them and store their ids in the database guaranteed that it
        // will not miss any.
        // Subsequent runs of the world can then verify that its required assets are present.
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
}

/// Sha1 hash of the asset archive
/// Stored as resource to hand to clients for verification
#[derive(Resource)]
pub struct AssetArchiveHash {
    pub hash: Vec<u8>,
}

fn make_asset_tarball(mut commands: Commands) {
    // If the assets have been changed, this will update tarball that is sent to the clients to reflect
    // the change.
    let possibly_changed_assets = build_asset_archive();

    if let Ok(saved_assets) = std::fs::read("resources/assets.tar") {
        // TODO: Should be able to add new assets to old worlds so you can update server and still
        // play on same world.
        if !is_same_sha1(&saved_assets, &possibly_changed_assets) {
            // Tarball doesn't match the asset directory (something added since last run)
            std::fs::write("resources/assets.tar", &possibly_changed_assets).unwrap();
        }
    } else {
        // Assets haven't been saved to a tarball yet
        std::fs::write("resources/assets.tar", &possibly_changed_assets).unwrap();
    }

    commands.insert_resource(AssetArchiveHash {
        hash: sha1::Sha1::digest(&possibly_changed_assets).to_vec(),
    });
}

// TODO: Should have some way for the client to download assets from an external location to reduce
// load on server.
fn handle_asset_requests(
    mut requests: EventReader<NetworkData<messages::AssetRequest>>,
    net: Res<NetworkServer>,
) {
    for request in requests.iter() {
        info!("sending assets");
        let asset_archive = std::fs::read("resources/assets.tar").unwrap();
        net.send_one(
            request.source,
            messages::AssetResponse {
                file: asset_archive,
            },
        )
    }
}

/// Check that none of the assets have changed since the last run.
fn is_same_sha1(archive_1: &Vec<u8>, archive_2: &Vec<u8>) -> bool {
    let hash_1 = sha1::Sha1::digest(&archive_1);
    let hash_2 = sha1::Sha1::digest(&archive_2);
    return hash_1 == hash_2;
}

/// Creates an archive from all the assets in the Assets directory
fn build_asset_archive() -> Vec<u8> {
    let mut archive = tar::Builder::new(Vec::new());
    archive
        .append_dir_all(".", "resources/client")
        .unwrap();
    return archive.into_inner().unwrap();
}
