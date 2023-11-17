use std::collections::HashMap;
use std::sync::Arc;

use bevy::prelude::*;
use fmc_networking::BlockId;
use noise::Noise;
use rand::SeedableRng;

use crate::{constants::CHUNK_SIZE, settings::Settings};

mod biomes;
mod features;

// The heighest point relative to the base height 3d noise can extend to create terrain.
const MAX_HEIGHT: i32 = 120;

pub struct TerrainGenerationPlugin;

impl Plugin for TerrainGenerationPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup);
    }
}

fn setup(mut commands: Commands, settings: Res<Settings>) {
    commands.insert_resource(TerrainGenerator::new(settings.seed));
}

#[derive(Resource, Deref, Clone)]
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

        let terrain_height = Noise::perlin(1./128., seed + 1)
            .fbm(5, 0.5, 2.0)
            // Increase so less of the terrain is flat
            .add_value(0.5)
            // Move to range 0.5..1.5, see application for how it works
            .clamp(0.0, 1.0)
            .add_value(0.5);

        // When out at sea bottom out the terrain height gradually from the shore, so big
        // landmasses don't poke out.
        let terrain_height = contintents.clone().range(0.0, -0.05, terrain_height, Noise::constant(0.5));

        let freq = 1.0/2.0f32.powi(8);
        let high = Noise::perlin(freq, seed + 2).with_frequency(freq, freq, freq).fbm(5, 0.5, 2.0);
        let low = Noise::perlin(freq, seed + 3).with_frequency(freq, freq, freq).fbm(5, 0.5, 2.0);

        // High and low are switched between to create sudden changes in terrain elevation.
        let freq = 1.0/92.0;
        let terrain_shape = Noise::perlin(0.0, seed + 4)
            .with_frequency(freq, freq * 0.5, freq)
            .fbm(4, 0.5, 2.0)
            .range(0.1, -0.1, high, low)
            .mul_value(1.5);

        Self(Arc::new(TerrainGeneratorInner {
            biome_map: biomes::BiomeMap::new(),
            continents: contintents,
            terrain_height,
            terrain_shape,
            seed,
        }))
    }
}

pub struct TerrainGeneratorInner {
    biome_map: biomes::BiomeMap,
    continents: Noise,
    terrain_height: Noise,
    terrain_shape: Noise,
    seed: i32,
}

impl TerrainGeneratorInner {
    // TODO: Use X direction of noise as Y direction, this way all access of the vector is
    // sequential, hopefully removing cache misses.
    //
    /// Generates all blocks for the chunk at the given position.
    /// Blocks that are generated outside of the chunk are also included (trees etc.)
    /// Return type (uniform, blocks), uniform if all blocks are of the same type.
    pub async fn generate_chunk(&self, chunk_position: IVec3) -> (bool, HashMap<IVec3, BlockId>) {
        let mut blocks: HashMap<IVec3, BlockId> = HashMap::with_capacity(CHUNK_SIZE.pow(3));

        // TODO: It should be unique to each chunk but I don't know how.
        // Seed used for feature placing, unique to each chunk column.
        let seed = self
            .seed
            .overflowing_add(chunk_position.x.pow(2))
            .0
            .overflowing_add(chunk_position.z)
            .0;
        let mut rng = rand::rngs::StdRng::seed_from_u64(seed as u64);

        // Don't waste time generating if it is guaranteed to be air.
        if MAX_HEIGHT < chunk_position.y {
            blocks.insert(chunk_position, self.biome_map.get_biome().air);
            return (true, blocks);
        }

        let biome = self.biome_map.get_biome();
        // TODO: Maybe when the frustum algo is moved to column based,
        // it's possible to move terrain generation to it too? Like you already know when
        // the chunk you want to generate is a surface chunk. idk...
        // Would remove need to generate these blocks.
        //
        // y_offset is the amount of blocks above the chunk that need to be generated to know how
        // deep we are, in order to know which blocks to use when at the surface.
        let y_offset = biome.top_layer_thickness + biome.mid_layer_thickness;

        // TODO: There's something going on here. Compression takes ~10 microseconds and terrain
        // shape takes 2 milliseconds. Terrain shape is 16 times larger than compression (same
        // amount of octaves).
        // After investigation: Switching from avx2 to sse2 seemed to alleviate it.
        let (mut terrain_shape, _, _) = self.terrain_shape.generate_3d(
            chunk_position.x as f32,
            chunk_position.y as f32,
            chunk_position.z as f32,
            CHUNK_SIZE,
            CHUNK_SIZE + y_offset,
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
                for y in 0..CHUNK_SIZE + y_offset {
                    // Amount the density should be decreased by per block above the base height
                    // for the maximum height to be MAX_HEIGHT.
                    // MAX_HEIGHT * DECREMENT / mounds_max = 1
                    const DECREMENT: f32 = 1.5 / MAX_HEIGHT as f32;
                    let mut compression =
                        ((chunk_position.y + y as i32) as f32 - base_height) * DECREMENT / terrain_height;
                    if compression < 0.0 {
                        compression *= 3.0;
                    }
                    let index = z * (CHUNK_SIZE * (CHUNK_SIZE + y_offset)) + y * CHUNK_SIZE + x;
                    terrain_shape[index] -= compression;
                }
            }
        }

        let mut uniform = true;
        let mut last_block = None;

        for x in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                let mut layer = 0;

                let base_height = base_height[z << 4 | x] * MAX_HEIGHT as f32;

                // Find how deep we are from above chunk.
                for y in CHUNK_SIZE..CHUNK_SIZE + y_offset {
                    // TODO: This needs to be converted to order xzy in simdnoise fork to make all
                    // access contiguous.
                    let block_index =
                        z * (CHUNK_SIZE * (CHUNK_SIZE + y_offset)) + y * CHUNK_SIZE + x;
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
                        z * (CHUNK_SIZE * (CHUNK_SIZE + y_offset)) + y * CHUNK_SIZE + x;
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
                    } else if layer > biome.mid_layer_thickness {
                        layer += 1;
                        biome.bottom_layer_block
                    } else if block_height < 2 && base_height < 2.0 {
                        layer += 1;
                        biome.sand
                    } else {
                        let block = if layer < biome.top_layer_thickness {
                            for feature_placer in biome.surface_features.iter() {
                                if let Some(feature) = feature_placer.place(
                                    chunk_position + IVec3::new(x as i32, y as i32, z as i32),
                                    &mut rng,
                                ) {
                                    for (block_position, block_id) in feature.into_iter() {
                                        // Generated feature overwrites air, but not solid blocks.
                                        blocks
                                            .entry(block_position)
                                            .and_modify(|block| {
                                                if *block == biome.air {
                                                    *block = block_id
                                                }
                                            })
                                            .or_insert(block_id);
                                    }
                                }
                            }
                            biome.top_layer_block
                        } else if layer < biome.mid_layer_thickness {
                            biome.mid_layer_block
                        } else {
                            biome.bottom_layer_block
                        };
                        layer += 1;
                        block
                    };

                    if last_block.is_none() {
                        last_block = Some(block);
                    } else if uniform && last_block.unwrap() != block {
                        uniform = false;
                    }

                    blocks
                        .entry(IVec3::new(
                            chunk_position.x + x as i32,
                            block_height,
                            chunk_position.z + z as i32,
                        ))
                        .or_insert(block);
                }
            }
        }

        return (uniform, blocks);
    }

    //pub fn get_surface_height(&self, x: i32, z: i32) -> i32 {
    //    return NoiseBuilder::fbm_2d_offset(x as f32, 1, z as f32, 1)
    //        .generate()
    //        .0[0]
    //        .round() as i32;
    //}
}
