use bevy::{
    prelude::*,
    render::{mesh::MeshVertexAttribute, render_resource::VertexFormat},
};

mod block_material;
mod pbr_material;
mod sky_material;

pub use block_material::BlockMaterial;
pub use pbr_material::PbrMaterial;
pub use sky_material::SkyMaterial;

pub const ATTRIBUTE_PACKED_BITS_0: MeshVertexAttribute =
    MeshVertexAttribute::new("Packed_bits_0", 10, VertexFormat::Uint32);

pub struct MaterialsPlugin;
impl Plugin for MaterialsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugin(MaterialPlugin::<BlockMaterial>::default())
            .add_plugin(MaterialPlugin::<SkyMaterial>::default())
            .add_plugin(pbr_material::PbrMaterialPlugin);
    }
}
