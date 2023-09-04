use bevy::prelude::*;

pub mod chunk;
mod lighting;
pub mod materials;
mod models;
mod sky;

pub struct RenderingPlugin;
impl Plugin for RenderingPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(materials::MaterialsPlugin)
            .add_plugins(chunk::ChunkPlugin)
            .add_plugins(lighting::LightingPlugin)
            .add_plugins(sky::SkyPlugin)
            .add_plugins(models::ModelPlugin);
    }
}
