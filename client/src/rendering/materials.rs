use bevy::prelude::*;

mod block_material;
pub mod sky_material;

pub use block_material::BlockMaterial;
pub use block_material::BLOCK_ATTRIBUTE_UV;
pub use sky_material::SkyMaterial;

pub struct MaterialsPlugin;
impl Plugin for MaterialsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugin(MaterialPlugin::<BlockMaterial>::default())
            .add_plugin(MaterialPlugin::<SkyMaterial>::default());
    }
}

//#[derive(Debug, Clone, TypeUuid)]
//#[uuid = "2ea7ea3b-5b0f-436b-a57b-d68b60c1c690"]
//struct FluidMaterial {}
//
//#[derive(Clone)]
//struct GpuFluidMaterial {}
//
//impl SpecializedMaterial for FluidMaterial {}
