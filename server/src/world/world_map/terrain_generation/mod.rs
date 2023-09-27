// There's a lot to do here. But I don't want to start anything without having it mapped out.
// Temporarily implemented just one biome.
// When implemented it should support a 3d biome system where biomes might change mid chunk.

use std::collections::HashMap;
use std::sync::Arc;

use bevy::prelude::*;
use fmc_networking::BlockId;
use rand::SeedableRng;
use simdnoise::NoiseBuilder;

use crate::{constants::CHUNK_SIZE, settings::ServerSettings};

mod biomes;
mod features;

// Used to determine the average height of the world.
const BASE_HEIGHT: f32 = 40.0;

// The heighest point relative to the base height 3d noise can extend to create terrain.
const MAX_RELATIVE_HEIGHT: f32 = 120.0;
// Same, but lowest, extends downwards.
const MIN_RELATIVE_HEIGHT: f32 = 120.0;

pub struct TerrainGenerationPlugin;

impl Plugin for TerrainGenerationPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, add_terrain_generator);
    }
}

fn add_terrain_generator(mut commands: Commands, server_settings: Res<ServerSettings>) {
    commands.insert_resource(TerrainGenerator::new(
        server_settings.seed,
    ));
}

#[derive(Resource, Deref, Clone)]
pub struct TerrainGenerator(Arc<TerrainGeneratorInner>);

impl TerrainGenerator {
    fn new(seed: u64) -> Self {
        Self(Arc::new(TerrainGeneratorInner {
            biome_map: biomes::BiomeMap::new(),
            seed,
        }))
    }
}

// TODO: It's terrible that the block ids need to be indexed by their name. Any name should be
// replaceable by an id at startup. Therefore there needs to be a way to be able to define a biome
// as data only, but I have no idea how. Like how would you define something like a tree that could
// have so many variations.
pub struct TerrainGeneratorInner {
    // TODO: Need some way to define this dynamically?
    biome_map: biomes::BiomeMap,
    // World seed
    seed: u64,
}

impl TerrainGeneratorInner {
    // 3d noise used to give shape to the terrain.
    fn terrain_shape_noise(&self, x: i32, y: i32, z: i32, y_offset: usize) -> Vec<f32> {
        const GAIN: f32 = 2.0;
        const OCTAVES: i32 = 3;
        let mut noise = NoiseBuilder::fbm_3d_offset(
            x as f32,
            CHUNK_SIZE,
            y as f32,
            CHUNK_SIZE + y_offset,
            z as f32,
            CHUNK_SIZE,
        )
        .with_gain(GAIN)
        .with_octaves(OCTAVES as u8)
        .with_freq(0.05)
        .generate()
        .0;

        let scale = (1.0 - GAIN.powi(OCTAVES)) / (1.0 - GAIN);
        noise.iter_mut().for_each(|x| *x /= scale);

        noise
    }

    // The base height of the terrain. Used to determine the general height of the world for the
    // entire chunk column.
    fn terrain_height_noise(&self, x: i32, z: i32) -> (Vec<f32>, f32, f32) {
        const GAIN: f32 = 2.0;
        const OCTAVES: i32 = 2;
        let mut noise = NoiseBuilder::fbm_2d_offset(x as f32, CHUNK_SIZE, z as f32, CHUNK_SIZE)
            .with_gain(GAIN)
            .with_octaves(OCTAVES as u8)
            .with_freq(0.001)
            .generate();

        let scale = (1.0 - GAIN.powi(OCTAVES)) / (1.0 - GAIN);
        noise.0.iter_mut().for_each(|x| *x /= scale);
        noise.1 /= scale;
        noise.2 /= scale;
        return noise;
    }

    fn compression_noise(&self, x: i32, z: i32) -> Vec<f32> {
        const GAIN: f32 = 2.0;
        const OCTAVES: i32 = 5;
        let mut noise = NoiseBuilder::ridge_2d_offset(x as f32, CHUNK_SIZE, z as f32, CHUNK_SIZE)
            .with_gain(GAIN)
            .with_octaves(OCTAVES as u8)
            .with_freq(0.04)
            .generate()
            .0;

        // TODO: Fork simdnoise and add this as another simd operation?
        // Scale to between -1 and 1
        let scale = (1.0 - GAIN.powi(OCTAVES)) / (1.0 - GAIN);
        noise.iter_mut().for_each(|x| *x /= scale);
        return noise;
    }

    fn compress_terrain_shape(
        terrain_shape: &mut Vec<f32>,
        compression: &Vec<f32>,
        terrain_height: &Vec<f32>,
        biomes: &Vec<&biomes::Biome>,
        chunk_position: IVec3,
        y_offset: usize,
    ) {
        for x in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                let index = z << 4 | x;
                let base_height = (terrain_height[index] * BASE_HEIGHT).round() as i32;
                // Shift from -1..1 to 0..1
                let column_compression = (1.0 + compression[index]) / 2.0;
                let biome = biomes[index];

                for y in 0..CHUNK_SIZE + y_offset {
                    let relative_height = (chunk_position.y + y as i32 - base_height) as f32;

                    let height_cap = match relative_height.is_sign_positive() {
                        true => column_compression * MAX_RELATIVE_HEIGHT,
                        false => column_compression * MIN_RELATIVE_HEIGHT,
                    };

                    // TODO: Maybe it makes sense to make this an exponential decay? Aggressive
                    // culling when far off will maybe leave fewer floating blocks.
                    let block_compression = (1.0 / height_cap).min(1.0);

                    let index = z * (CHUNK_SIZE * (CHUNK_SIZE + y_offset)) + y * CHUNK_SIZE + x;
                    let density = &mut terrain_shape[index];

                    // Notice relative_height carries the sign.
                    *density -= block_compression * relative_height;
                }
            }
        }
    }

    // TODO: Use X direction of noise as Y direction, this way all access of the vector is
    // sequential, hopefully removing cache misses.
    //
    // Terrain is calculated through a base of 3d noise. This noise is manipulated through a set
    // of 2d noises. The first manipulator is 'terrain height', it moves the base height of the
    // terrain up and down by using it as a middle point on the y axis of the 3d noise. It is
    // clamped between the MAX_TERRAIN_HEIGHT and MIN_TERRAIN_HEIGHT.
    // The second is 'compression', it acts as a second lever on how high the terrain
    // should be. It's value is used as as density modifier. Terrain above the base terrain height
    // has its density decreased, and below, increased, depending on how for away they are from the
    // base terrain height, farther == more. Example: A compression value of 0.5 and a block
    // above the base terrain. First we find the value's ratio of MAX_RELATIVE_HEIGHT, 100 say, this
    // would be 50. This means we don't want any blocks above base_terrain_height+50. To accomplish
    // this we divide -1 by 50 to get the density modifier per block height increase. Any given
    // block will then have its height difference relative to the base height multiplied by the
    // density modifier and added to its density. This way a block 50 blocks above will have its
    // density decreased by '50 * -1/50 = -1', and with the max density being 1, never be visible.
    // Inverted for all blocks below using MIN_RELATIVE_HEIGHT and 1 instead of -1.
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
            .overflowing_add((chunk_position.x as u64).overflowing_mul(i32::MAX as u64).0)
            .0
            .overflowing_add(chunk_position.z as u64)
            .0;
        let mut rng = rand::rngs::StdRng::seed_from_u64(seed);

        let (terrain_height, _min, max) =
            self.terrain_height_noise(chunk_position.x, chunk_position.z);

        // Don't waste time generating if it is guaranteed to be air.
        if max * BASE_HEIGHT + MAX_RELATIVE_HEIGHT < chunk_position.y as f32 {
            blocks.insert(chunk_position, self.biome_map.get_biome().air);
            return (true, blocks);
        }

        // y_offset is the amount of blocks above the chunk that need to be generated. These are
        // needed to determine how deep the chunk's blocks are. I don't think there's any easy way
        // to do this since it's all 3d noise, no terrain height to read from.
        let mut y_offset = 0;

        let biomes: Vec<&biomes::Biome> = terrain_height
            .iter()
            .map(|_| {
                let biome = self.biome_map.get_biome();
                y_offset = y_offset.max(biome.top_layer_thickness + biome.mid_layer_thickness);
                biome
            })
            .collect();

        // TODO: There's something going on here. Compression takes ~10 microseconds and terrain
        // shape takes 2 milliseconds. Terrain shape is 16 times larger than compression (same
        // amount of octaves).
        // After investigation: Switching from avx2 to sse2 seemed to alleviate it.
        let mut terrain_shape = self.terrain_shape_noise(
            chunk_position.x,
            chunk_position.y,
            chunk_position.z,
            y_offset,
        );
        let compression = self.compression_noise(chunk_position.x, chunk_position.z);

        Self::compress_terrain_shape(
            &mut terrain_shape,
            &compression,
            &terrain_height,
            &biomes,
            chunk_position,
            y_offset,
        );

        let mut uniform = true;
        let mut last_block = None;

        for x in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                let biome = biomes[z << 4 | x];

                let mut layer = 0;

                // Find how deep we are already.
                for y in CHUNK_SIZE..CHUNK_SIZE + y_offset {
                    // TODO: This needs to be converted to order xzy in simdnoise fork to make all
                    // access contiguous.
                    let block_index =
                        z * (CHUNK_SIZE * (CHUNK_SIZE + y_offset)) + y * CHUNK_SIZE + x;
                    let density = terrain_shape[block_index];

                    if density < 0.0 {
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

                    let block = if density < 0.0 {
                        layer = 0;
                        if block_height == 0 {
                            biome.surface_liquid
                        } else if block_height < 0 {
                            biome.sub_surface_liquid
                        } else {
                            biome.air
                        }
                    } else if block_height < 1 {
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

    pub fn get_surface_height(&self, x: i32, z: i32) -> i32 {
        return NoiseBuilder::fbm_2d_offset(x as f32, 1, z as f32, 1)
            .generate()
            .0[0]
            .round() as i32;
    }
}
