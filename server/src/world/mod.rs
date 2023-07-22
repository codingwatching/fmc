use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{database::DatabaseArc, settings::ServerSettings};

use self::world_map::terrain_generation::TerrainGeneratorArc;

/// Block properties
// TODO: limit scope?
pub mod blocks;
/// Manages the items
pub mod items;
/// Keeps track of models sent to the client.
pub mod models;
mod sky;
/// Stores the world map and handles changes.
pub mod world_map;

pub struct WorldPlugin;
impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugin(blocks::BlockPlugin)
            .add_plugin(items::ItemPlugin)
            .add_plugin(models::ModelPlugin)
            .add_plugin(world_map::WorldMapPlugin)
            .add_plugin(sky::SkyPlugin)
            .add_systems(PreStartup, load_world_properties)
            .add_systems(
                Update,
                save_world_properties.run_if(resource_changed::<WorldProperties>()),
            );
    }
}

fn load_world_properties(mut commands: Commands, database: Res<DatabaseArc>) {
    let properties = if let Some(properties) = database.load_world_properties() {
        properties
    } else {
        WorldProperties::default()
    };

    commands.insert_resource(properties);
}

fn save_world_properties(database: Res<DatabaseArc>, properties: Res<WorldProperties>) {
    database.save_world_properties(&properties);
}

#[derive(Default, Serialize, Deserialize, Resource)]
pub struct WorldProperties {
    // TODO: This must be set to a valid spawn point when first inserted, currently it is just
    // ignored.
    pub spawn_point: SpawnPoint,
}

/// The default spawn point, as opposed to the unique spawn point of a player.
#[derive(Default, Serialize, Deserialize)]
pub struct SpawnPoint {
    pub center: IVec3,
    pub radius: i32,
}
