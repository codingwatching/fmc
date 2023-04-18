use std::sync::Arc;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{
    database::{Database, DatabaseArc},
    settings::ServerSettings,
};

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
            .add_systems(Startup, load_world_properties);
    }
}

fn load_world_properties(
    mut commands: Commands,
    database: Res<DatabaseArc>,
    terrain_generator: Res<TerrainGeneratorArc>,
) {
    let properties = if let Some(properties) = database.load_world_properties() {
        properties
    } else {
        let properties = WorldProperties {
            // TODO: Change when implementing proper generation
            spawn_point: SpawnPoint {
                center: IVec3::new(0, terrain_generator.get_surface_height(0, 0), 0),
                radius: 10,
            },
        };
        database.save_world_properties(&properties);

        properties
    };

    commands.insert_resource(properties);
}

#[derive(Serialize, Deserialize, Resource)]
pub struct WorldProperties {
    spawn_point: SpawnPoint,
}

// TODO: This needs a system to find a new spawn point every time a player is respawned.
/// Ressource used to set the spawn point for new players.
/// Defaults to 0,y,0, where y is the height level of the overworld terrain.
#[derive(Serialize, Deserialize)]
pub struct SpawnPoint {
    pub center: IVec3,
    pub radius: i32,
}
