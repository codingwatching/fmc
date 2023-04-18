use bevy::prelude::*;

pub mod chunk;
pub mod materials;
mod models;

pub struct RenderingPlugin;
impl Plugin for RenderingPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugin(materials::MaterialsPlugin)
            .add_plugin(chunk::ChunkPlugin)
            .add_plugin(models::ModelPlugin);
    }
}
