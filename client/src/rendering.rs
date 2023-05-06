use bevy::prelude::*;

pub mod chunk;
mod lighting;
pub mod materials;
mod models;
mod sky;

pub struct RenderingPlugin;
impl Plugin for RenderingPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugin(materials::MaterialsPlugin)
            .add_plugin(chunk::ChunkPlugin)
            .add_plugin(lighting::LightingPlugin)
            .add_plugin(sky::SkyPlugin)
            .add_plugin(models::ModelPlugin);
    }
}
