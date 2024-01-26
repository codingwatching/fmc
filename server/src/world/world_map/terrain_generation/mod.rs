use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use bevy::prelude::*;
use fmc_networking::BlockId;
use noise::Noise;
use rand::SeedableRng;

use crate::world::blocks::Blocks;
use crate::{constants::CHUNK_SIZE, settings::Settings, utils, world::blocks::BlockState};

use super::chunk::Chunk;

mod biomes;
mod blueprints;

// The heighest point relative to the base height 3d noise can extend to create terrain.
const MAX_HEIGHT: i32 = 120;

// y_offset is the amount of blocks above the chunk that need to be generated to know how
// deep we are, in order to know which blocks to use when at the surface.
const Y_OFFSET: usize = 4;

pub struct TerrainGenerationPlugin;

impl Plugin for TerrainGenerationPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup);
    }
}

fn setup(mut commands: Commands, settings: Res<Settings>) {
    commands.insert_resource(TerrainGenerator::new(settings.seed));
}

#[derive(Resource, Clone)]
pub struct TerrainGenerator(Arc<TerrainGeneratorInner>);

impl TerrainGenerator {
    fn new(seed: i32) -> Self {
        //let freq = 1.0/200.0;
        //let terrain_low = Noise::simplex(0.0, seed).with_frequency(freq, 0.0, freq).fbm(4, 0.5, 2.0).mul_value(0.3);
        //let terrain_high = Noise::simplex(0.0, seed + 1).with_frequency(freq, 0.0, freq).fbm(4, 0.5, 2.0).max(terrain_low.clone());
        //let freq = 1.0/200.0;
        //let terrain_shape_low = Noise::simplex(0.0, seed + 2).with_frequency(freq, freq * 0.5, freq).fbm(5, 0.5, 2.0);
        //let terrain_shape_high = Noise::simplex(0.0, seed + 3).with_frequency(freq, freq * 0.5, freq).fbm(5, 0.5, 2.0);
        //let terrain_shape = Noise::simplex(0.0, seed + 4).with_frequency(freq, freq * 0.5, freq).lerp(terrain_shape_high, terrain_shape_low).range(0.1, -0.1, terrain_high, terrain_low);

        // ANOTHER ATTEMPT
        //let freq = 0.002;
        //let base_terrain = Noise::simplex(0.0, seed).with_frequency(freq, 0.0, freq).fbm(4, 0.5, 2.0).mul_value(0.1);
        //let freq = 0.003;
        //let mound = Noise::simplex(0.0, seed + 1).with_frequency(freq, 0.0, freq).fbm(4, 0.5, 2.0).abs().mul_value(0.3).add(base_terrain.clone()).max(base_terrain.clone());
        ////let mounds = Noise::simplex(0.005, seed + 3).fbm(4, 0.5, 2.0).range(0.5, -0.5, mound_high.clone(), mound_low.clone());

        ////let terrain_low = Noise::simplex(0.001, seed + 4).fbm(6, 0.5, 2.0).add(base_terrain.clone());
        ////let terrain_high = Noise::simplex(0.005, seed + 5).fbm(4, 0.5, 2.0).range(0.5, -0.5, mound_high, mound_low).add(base_terrain).add_value(0.5);

        //let freq = 1.0/150.0;
        //let terrain_shape = Noise::simplex(0.0, seed + 6).with_frequency(freq, freq * 0.5, freq).fbm(5, 0.5, 2.0);
        //let terrain_shape = terrain_shape.clone().range(0.5, -0.0, mound.clone(), base_terrain);
        ////let terrain_shape = terrain_shape.range(0.8, 0.7, mound.clone().add_value(0.4), terrain_shape_low);

        let contintents = Noise::perlin(0.005, seed)
            .fbm(6, 0.5, 2.0)
            // Increase so less of the world is sea
            .add_value(0.25)
            // Reduce height of contintents to be between -10%/5% of MAX_HEIGHT
            .clamp(-0.1, 0.05);

        let terrain_height = Noise::perlin(1. / 128., seed + 1)
            .fbm(5, 0.5, 2.0)
            // Increase so less of the terrain is flat
            .add_value(0.5)
            // Move to range 0.5..1.5, see application for how it works
            .clamp(0.0, 1.0)
            .add_value(0.5);

        // When out at sea bottom out the terrain height gradually from the shore, so big
        // landmasses don't poke out.
        let terrain_height =
            contintents
                .clone()
                .range(0.0, -0.05, terrain_height, Noise::constant(0.5));

        let freq = 1.0 / 2.0f32.powi(8);
        let high = Noise::perlin(freq, seed + 2)
            .with_frequency(freq, freq, freq)
            .fbm(4, 0.5, 2.0);
        let low = Noise::perlin(freq, seed + 3)
            .with_frequency(freq, freq, freq)
            .fbm(4, 0.5, 2.0);

        // High and low are switched between to create sudden changes in terrain elevation.
        //let freq = 1.0/92.0;
        let freq = 1.0 / 2.0f32.powi(9);
        let terrain_shape = Noise::perlin(0.0, seed + 4)
            .with_frequency(freq, freq, freq)
            .fbm(8, 0.5, 2.0)
            .range(0.1, -0.1, high, low)
            .mul_value(2.0);

        Self(Arc::new(TerrainGeneratorInner {
            biomes: biomes::Biomes::load(),
            continents: contintents,
            terrain_height,
            terrain_shape,
            seed,
        }))
    }

    pub async fn generate_chunk(&self, chunk_position: IVec3, chunk: &mut Chunk) {
        self.0.generate_chunk(chunk_position, chunk);
    }
}

struct TerrainGeneratorInner {
    biomes: biomes::Biomes,
    continents: Noise,
    terrain_height: Noise,
    terrain_shape: Noise,
    seed: i32,
}

impl TerrainGeneratorInner {
    fn generate_chunk(&self, chunk_position: IVec3, chunk: &mut Chunk) {
        // Don't waste time generating if it is guaranteed to be air.
        if MAX_HEIGHT < chunk_position.y {
            let air = Blocks::get().get_id("air");
            chunk.make_uniform(air);
        } else {
            self.generate_terrain(chunk_position, chunk);
            self.carve_caves(chunk);
            self.generate_features(chunk_position, chunk);
        }

        chunk.check_visible_faces();
    }

    fn generate_terrain(&self, chunk_position: IVec3, chunk: &mut Chunk) {
        let (mut terrain_shape, _, _) = self.terrain_shape.generate_3d(
            chunk_position.x as f32,
            chunk_position.y as f32,
            chunk_position.z as f32,
            CHUNK_SIZE,
            CHUNK_SIZE + Y_OFFSET,
            CHUNK_SIZE,
        );

        let (base_height, _, _) = self.continents.generate_2d(
            chunk_position.x as f32,
            chunk_position.z as f32,
            CHUNK_SIZE,
            CHUNK_SIZE,
        );

        let (terrain_height, _, _) = self.terrain_height.generate_2d(
            chunk_position.x as f32,
            chunk_position.z as f32,
            CHUNK_SIZE,
            CHUNK_SIZE,
        );

        for x in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                let index = z << 4 | x;
                let base_height = base_height[index] * MAX_HEIGHT as f32;
                let terrain_height = terrain_height[index];
                for y in 0..CHUNK_SIZE + Y_OFFSET {
                    // Amount the density should be decreased by per block above the base height
                    // for the maximum height to be MAX_HEIGHT.
                    // MAX_HEIGHT * DECREMENT / mounds_max = 1
                    const DECREMENT: f32 = 1.5 / MAX_HEIGHT as f32;
                    let mut compression = ((chunk_position.y + y as i32) as f32 - base_height)
                        * DECREMENT
                        / terrain_height;
                    if compression < 0.0 {
                        compression *= 3.0;
                    }
                    let index = z * (CHUNK_SIZE * (CHUNK_SIZE + Y_OFFSET)) + y * CHUNK_SIZE + x;
                    terrain_shape[index] -= compression;
                }
            }
        }

        chunk.blocks = vec![0; CHUNK_SIZE.pow(3)];

        let biome = self.biomes.get_biome();

        // TODO: This should actually be "is_uniform"
        let mut is_air = true;

        for x in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                let mut layer = 0;

                let base_height = base_height[z << 4 | x] * MAX_HEIGHT as f32;

                // Find how deep we are from above chunk.
                for y in CHUNK_SIZE..CHUNK_SIZE + Y_OFFSET {
                    // TODO: This needs to be converted to order xzy in simdnoise fork to make all
                    // access contiguous.
                    let block_index =
                        z * (CHUNK_SIZE * (CHUNK_SIZE + Y_OFFSET)) + y * CHUNK_SIZE + x;
                    let density = terrain_shape[block_index];

                    if density <= 0.0 {
                        if chunk_position.y + y as i32 <= 0 {
                            // For water
                            layer = 1;
                        }
                        break;
                    } else {
                        layer += 1;
                    }
                }

                for y in (0..CHUNK_SIZE).rev() {
                    let block_height = chunk_position.y + y as i32;

                    let block_index =
                        z * (CHUNK_SIZE * (CHUNK_SIZE + Y_OFFSET)) + y * CHUNK_SIZE + x;
                    let density = terrain_shape[block_index];

                    let block = if density <= 0.0 {
                        if block_height == 0 {
                            layer = 1;
                            biome.surface_liquid
                        } else if block_height < 0 {
                            layer = 1;
                            biome.sub_surface_liquid
                        } else {
                            layer = 0;
                            biome.air
                        }
                    } else if layer > 3 {
                        layer += 1;
                        biome.bottom_layer_block
                    } else if block_height < 2 && base_height < 2.0 {
                        layer += 1;
                        biome.sand
                    } else {
                        let block = if layer < 1 {
                            biome.top_layer_block
                        } else if layer < 3 {
                            biome.mid_layer_block
                        } else {
                            biome.bottom_layer_block
                        };
                        layer += 1;
                        block
                    };

                    if is_air && biome.air != block {
                        is_air = false;
                    }

                    chunk[[x, y, z]] = block;
                }
            }
        }

        if is_air {
            chunk.make_uniform(biome.air);
        }
    }

    fn carve_caves(&self, chunk: &mut Chunk) {}

    fn generate_features(&self, chunk_position: IVec3, chunk: &mut Chunk) {
        // TODO: It should be unique to each chunk but I don't know how.
        let seed = self
            .seed
            .overflowing_add(chunk_position.x.pow(2))
            .0
            .overflowing_add(chunk_position.z)
            .0;
        let mut rng = rand::rngs::StdRng::seed_from_u64(seed as u64);

        let air = Blocks::get().get_id("air");

        // TODO: This should be done at terrain generation, but it clutters the code and it's in
        // flux. Meanwhile it is done here; an entire extra scan of the chunk, and it can't tell
        // if it's the surface if it's the topmost block in a column.
        //
        // The surface contains the first block from the top that is not air for each block column
        // of the chunk.
        let mut surface = vec![None; CHUNK_SIZE.pow(2)];
        for (column_index, block_column) in chunk.blocks.chunks(CHUNK_SIZE).enumerate() {
            let mut air_encountered = false;
            for (y_index, block_id) in block_column.into_iter().enumerate().rev() {
                if air_encountered && *block_id != air {
                    // The 2d surface stores the index in the 3d chunk and the block. The
                    // bitshifting just converts it to a chunk index. See 'Chunk::Index' if
                    // wondering what it means.
                    surface[column_index] = Some((y_index, *block_id));
                    break;
                }
                if *block_id == air {
                    air_encountered = true;
                }
            }
        }

        let biome = self.biomes.get_biome();

        for blueprint in biome.blueprints.iter() {
            let terrain_feature = blueprint.construct(chunk_position, &surface, &mut rng);

            if terrain_feature.blocks.is_empty() {
                continue;
            }

            terrain_feature.apply(chunk, chunk_position);

            chunk.terrain_features.push(terrain_feature);
        }
    }
}

pub struct TerrainFeature {
    // The blocks the feature consists of segmented into the chunks they are a part of.
    pub blocks: HashMap<IVec3, Vec<(usize, BlockId, Option<u16>)>>,
    // TODO: Replacement rules should be more granular. Blueprints consist of many sub-blueprints that
    // each have their own replacement rules that should be followed only for that blueprint.
    pub can_replace: HashSet<BlockId>,
}

impl TerrainFeature {
    fn insert_block(&mut self, position: IVec3, block_id: BlockId) {
        let (chunk_position, block_index) =
            utils::world_position_to_chunk_position_and_block_index(position);
        self.blocks
            .entry(chunk_position)
            .or_insert(Vec::new())
            .push((block_index, block_id, None));
    }

    // TODO: check for failure and return bool
    pub fn apply(&self, chunk: &mut Chunk, chunk_position: IVec3) {
        if let Some(feature_blocks) =
            self.blocks.get(&chunk_position)
        {
            for (block_index, block_id, block_state) in feature_blocks {
                if !chunk.changed_blocks.contains_key(block_index) {
                    chunk[*block_index] = *block_id;
                    chunk.set_block_state(
                        *block_index,
                        block_state.map(BlockState),
                    );
                }
            }
        }
    }

    pub fn apply_return_changed(&self, chunk: &mut Chunk, chunk_position: IVec3) -> Option<Vec<(usize, BlockId, Option<u16>)>> {
        if let Some(mut feature_blocks) =
            self.blocks.get(&chunk_position).cloned()
        {
            feature_blocks.retain(|(block_index, block_id, block_state)| {
                if !chunk.changed_blocks.contains_key(block_index) {
                    chunk[*block_index] = *block_id;
                    chunk.set_block_state(
                        *block_index,
                        block_state.map(BlockState),
                    );
                    true
                } else {
                    false
                }
            });
            return Some(feature_blocks);
        } else {
            return None;
        }
    }

}
