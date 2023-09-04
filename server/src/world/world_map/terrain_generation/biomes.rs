use fmc_networking::BlockId;

use crate::world::blocks::Blocks;

use super::features::{tree::Tree, FeaturePlacer};

pub struct Biome {
    pub top_layer_block: BlockId,
    pub top_layer_thickness: usize,
    pub mid_layer_block: BlockId,
    pub mid_layer_thickness: usize,
    pub bottom_layer_block: BlockId,
    pub surface_features: Vec<FeaturePlacer>,
    pub surface_liquid: BlockId,
    pub sub_surface_liquid: BlockId,
    pub air: BlockId,
    pub sand: BlockId,
}

// TODO: Create dynamically so it's easier to change. Should be able to add biomes between
// intervals and error if they overlap.
pub struct BiomeMap {
    biomes: [Biome; 1],
}

impl BiomeMap {
    pub fn new() -> Self {
        let blocks = Blocks::get();
        let forest = Biome {
            top_layer_block: blocks.get_id("grass"),
            top_layer_thickness: 1,
            mid_layer_block: blocks.get_id("dirt"),
            mid_layer_thickness: 3,
            bottom_layer_block: blocks.get_id("stone"),
            surface_features: vec![FeaturePlacer::new(
                3,
                Box::new(Tree::new(blocks.get_id("oak"), blocks.get_id("leaves"))),
            )],
            surface_liquid: blocks.get_id("surface_water"),
            sub_surface_liquid: blocks.get_id("subsurface_water"),
            air: blocks.get_id("air"),
            sand: blocks.get_id("sand"),
        };
        return Self { biomes: [forest] };
    }

    pub fn get_biome(&self) -> &Biome {
        return &self.biomes[0];
    }
}
