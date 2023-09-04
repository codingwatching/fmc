use bevy::prelude::*;

pub mod chunk;
mod chunk_manager;
mod world_map;

pub use world_map::WorldMap;

pub struct WorldMapPlugin;
impl Plugin for WorldMapPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(chunk_manager::ChunkManagerPlugin)
            .add_plugins(chunk::ChunkPlugin)
            .init_resource::<WorldMap>();
    }
}
