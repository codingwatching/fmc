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

// TODO: Read this from biome
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

        let freq = 0.005;
        let continents = Noise::perlin(freq, seed)
            .with_frequency(freq, 0.0, freq)
            .fbm(6, 0.5, 2.0)
            // Increase so less of the world is sea
            .add_value(0.25)
            // Reduce height of contintents to be between -10%/5% of MAX_HEIGHT
            .clamp(-0.1, 0.05);

        let freq = 1.0 / 128.0;
        let terrain_height = Noise::perlin(freq, seed + 1)
            .with_frequency(freq, 0.0, freq)
            .fbm(5, 0.5, 2.0)
            // Increase so less of the terrain is flat
            .add_value(0.5)
            // Move to range 0.5..1.5, see application for how it works
            .clamp(0.0, 1.0)
            .add_value(0.5);

        // When out at sea bottom out the terrain height gradually from the shore, so big
        // landmasses don't poke out.
        let terrain_height =
            continents
                .clone()
                .range(0.0, -0.05, terrain_height, Noise::constant(0.5));

        let freq = 1.0 / 2.0f32.powi(8);
        let high = Noise::perlin(freq, seed + 2).fbm(4, 0.5, 2.0);
        let low = Noise::perlin(freq, seed + 3).fbm(4, 0.5, 2.0);

        // High and low are switched between to create sudden changes in terrain elevation.
        //let freq = 1.0/92.0;
        let freq = 1.0 / 2.0f32.powi(9);
        let terrain_shape = Noise::perlin(freq, seed + 4)
            .fbm(8, 0.5, 2.0)
            .range(0.1, -0.1, high, low)
            .mul_value(2.0);

        // This is a failed attempt at making snaking tunnels. The idea is to generate 2d noise,
        // abs it, then use the values under some threshold as the direction of the tunnels. To
        // translate it into 3d, a 3d noise is generated through the same procedure, and overlayed
        // on the 2d noise. When you take the absolute value of 3d noise and threshold it, it
        // creates sheets, instead of lines. The overlay between the sheets and the lines of the 2d
        // noise create the tunnels, where the 2d noise effectively constitute the range
        // between the horizontal walls, and the 3d noise the range between the vertical walls.
        //
        // The big problems with this approach is one, no matter which depth you're at, the 2d noise
        // stays the same, and two, the 3d noise creates vertical walls when it changes direction,
        // when the 2d noise is parallel with these walls, it creates really tall narrow
        // unwalkable crevices.
        //
        //let freq = 0.004;
        //let tunnels = Noise::perlin(0.0, seed + 5)
        //    .with_frequency(freq * 2.0, freq * 2.0, freq * 2.0)
        //    .abs()
        //    .max(
        //        Noise::simplex(0.00, seed + 6)
        //            .with_frequency(freq, 0.0, freq)
        //            .abs()
        //    );

        // Visualization: https://www.shadertoy.com/view/stccDB
        let freq = 0.01;
        let cave_main = Noise::perlin(freq, seed + 5)
            .with_frequency(freq, freq * 2.0, freq)
            .fbm(3, 0.5, 2.0)
            .square();
        let cave_main_2 = Noise::perlin(freq, seed + 6)
            .with_frequency(freq, freq * 2.0, freq)
            .fbm(3, 0.5, 2.0)
            .square();
        let caves = continents.clone().range(
            // TODO: These numbers are slightly below the continents max because I implemented
            // range as non-inclusive.
            0.049,
            0.049,
            cave_main.add(cave_main_2),
            Noise::constant(1.0),
        );

        Self(Arc::new(TerrainGeneratorInner {
            biomes: biomes::Biomes::load(),
            continents,
            terrain_height,
            terrain_shape,
            caves,
            seed,
        }))
    }

    // TODO: This takes ~1ms, way too slow. The simd needs to be inlined, the function call
    // overhead is 99% of the execution time I'm guessing. When initially benchmarking the noise
    // lib I remember using a simple 'add(some_value)' spiked execution time by 33/50%, it
    // corresponded to 1 extra simd instruction, compared to the hundreds of instructions of the
    // noise it is applied to.
    pub fn generate_chunk(&self, chunk_position: IVec3, chunk: &mut Chunk) {
        self.0.generate_chunk(chunk_position, chunk);
    }
}

struct TerrainGeneratorInner {
    biomes: biomes::Biomes,
    continents: Noise,
    terrain_height: Noise,
    terrain_shape: Noise,
    caves: Noise,
    seed: i32,
}

impl TerrainGeneratorInner {
    fn generate_chunk(&self, chunk_position: IVec3, chunk: &mut Chunk) {
        let air = Blocks::get().get_id("air");
        if MAX_HEIGHT < chunk_position.y {
            // Don't waste time generating if it is guaranteed to be air.
            chunk.make_uniform(air);
        } else {
            self.generate_terrain(chunk_position, chunk);

            // TODO: Might make sense to test against water too.
            //
            // Test for air chunk uniformity early so we can break and elide the other generation
            // functions. This makes it so all other chunks that are uniform with another type of
            // block get stored as full size chunks. They are assumed to be very rare.
            let mut uniform = true;
            for block in chunk.blocks.iter() {
                if *block != air {
                    uniform = false;
                    break;
                }
            }

            if uniform {
                chunk.make_uniform(air);
                chunk.check_visible_faces();
                return;
            }

            self.carve_caves(chunk_position, chunk);
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

        let (base_height, _, _) = self.continents.generate_3d(
            chunk_position.x as f32,
            0.0,
            chunk_position.z as f32,
            CHUNK_SIZE,
            1,
            CHUNK_SIZE,
        );

        let (terrain_height, _, _) = self.terrain_height.generate_3d(
            chunk_position.x as f32,
            0.0,
            chunk_position.z as f32,
            CHUNK_SIZE,
            1,
            CHUNK_SIZE,
        );

        for x in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                let index = x << 4 | z;
                let base_height = base_height[index] * MAX_HEIGHT as f32;
                let terrain_height = terrain_height[index];
                for y in 0..CHUNK_SIZE + Y_OFFSET {
                    // Amount the density should be decreased by per block above the base height
                    // for the maximum height to be MAX_HEIGHT.
                    // MAX_HEIGHT * DECREMENT / terrain_height_max = 1
                    const DECREMENT: f32 = 1.5 / MAX_HEIGHT as f32;
                    let mut compression = ((chunk_position.y + y as i32) as f32 - base_height)
                        * DECREMENT
                        / terrain_height;
                    if compression < 0.0 {
                        // Below surface, extra compression
                        compression *= 3.0;
                    }
                    let index = x * (CHUNK_SIZE * (CHUNK_SIZE + Y_OFFSET))
                        + z * (CHUNK_SIZE + Y_OFFSET)
                        + y;
                    // Decrease density if above base height, increase if below
                    terrain_shape[index] -= compression;
                }
            }
        }

        chunk.blocks = vec![0; CHUNK_SIZE.pow(3)];

        let biome = self.biomes.get_biome();

        for x in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                let mut layer = 0;

                let base_height = base_height[x << 4 | z] * MAX_HEIGHT as f32;

                // Find how deep we are from above chunk.
                for y in CHUNK_SIZE..CHUNK_SIZE + Y_OFFSET {
                    // TODO: This needs to be converted to order xzy in simdnoise fork to make all
                    // access contiguous.
                    let block_index = x * (CHUNK_SIZE * (CHUNK_SIZE + Y_OFFSET))
                        + z * (CHUNK_SIZE + Y_OFFSET)
                        + y;
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

                    let block_index = x * (CHUNK_SIZE * (CHUNK_SIZE + Y_OFFSET))
                        + z * (CHUNK_SIZE + Y_OFFSET)
                        + y;
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

                    chunk[[x, y, z]] = block;
                }
            }
        }
    }

    fn carve_caves(&self, chunk_position: IVec3, chunk: &mut Chunk) {
        let air = Blocks::get().get_id("air");

        let biome = self.biomes.get_biome();
        let (caves, _, _) = self.caves.generate_3d(
            chunk_position.x as f32,
            chunk_position.y as f32,
            chunk_position.z as f32,
            CHUNK_SIZE,
            CHUNK_SIZE,
            CHUNK_SIZE,
        );
        caves
            .into_iter()
            .zip(chunk.blocks.iter_mut())
            .enumerate()
            .for_each(|(i, (mut density, block))| {
                // TODO: Caves and water do not cooperate well. You carve the surface without
                // knowing there's water there and you get reverse moon pools underwater. Instead
                // we just push the caves underground, causing there to be no cave entraces at the
                // surface. There either needs to be a way to exclude caves from being generated
                // beneath water, or some way to intelligently fill carved out space that touches
                // water.
                const DECAY_POINT: i32 = -32;
                let y = chunk_position.y + (i & 0b1111) as i32;
                let density_offset = (y - DECAY_POINT).max(0) as f32 * 1.0 / 64.0;
                density += density_offset;

                if (density / 2.0) < 0.001
                    && *block != biome.surface_liquid
                    && *block != biome.sub_surface_liquid
                {
                    *block = air;
                }
            });
    }

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
        // flux. Meanwhile, it is done here. An entire extra scan of the chunk, and it can't tell
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
    /// The blocks the feature consists of segmented into the chunks they are a part of.
    pub blocks: HashMap<IVec3, Vec<(usize, BlockId, Option<u16>)>>,
    // TODO: Replacement rules should be more granular. Blueprints may consist of many
    // sub-blueprints that each have their own replacement rules that should be followed only for
    // that blueprint.
    // TODO: This is really inefficient. Most features will match against a single block like air
    // or stone, it doesn't make sense to do a lookup, might even be best to do linear search.
    // Enum {
    //     Any, Can replace anything
    //     Single(BlockId), Can replace a single block, fast comparison
    //     Afew([Option<BlockId>; 5]), If there's 2-5 replace rules this is probably faster to search
    //     Many(Hashset<BlockId>), If there are more, benchmark length when faster to do lookup
    //     than search the above.
    //     Magic(...), You probably want some way to do "if replacing this block, use that block",
    //     like ores for different types of stone.
    // }
    // https://gist.github.com/daboross/976978d8200caf86e02acb6805961195 says really long at bottom
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

    // TODO: Is it possible to make it so that features can fail? There are many things that just
    // don't look very good when partially placed. Failure means it would have to revert to the
    // previous state, which is not an easy task. The features are applied to chunks as the chunks
    // are generated, and changing the block to then set it back again does not seem plausible.
    // There would have to be some notification system I suppose that triggers a feature
    // application when all the chunks it will apply to have been generated and are in memory. Then
    // it can check all placements as the first thing it does, then apply if it succeeds. Sounds
    // expensive though.
    pub fn apply(&self, chunk: &mut Chunk, chunk_position: IVec3) {
        if let Some(feature_blocks) = self.blocks.get(&chunk_position) {
            for (block_index, block_id, block_state) in feature_blocks {
                if !chunk.changed_blocks.contains_key(block_index)
                    && self.can_replace.contains(&chunk[*block_index])
                {
                    chunk[*block_index] = *block_id;
                    chunk.set_block_state(*block_index, block_state.map(BlockState));
                }
            }
        }
    }

    // Applies the feature and returns the blocks that were changed. Used for updating chunks that
    // have already been sent to the clients.
    pub fn apply_return_changed(
        &self,
        chunk: &mut Chunk,
        chunk_position: IVec3,
    ) -> Option<Vec<(usize, BlockId, Option<u16>)>> {
        if let Some(mut feature_blocks) = self.blocks.get(&chunk_position).cloned() {
            feature_blocks.retain(|(block_index, block_id, block_state)| {
                if !chunk.changed_blocks.contains_key(block_index)
                    && self.can_replace.contains(&chunk[*block_index])
                {
                    chunk[*block_index] = *block_id;
                    chunk.set_block_state(*block_index, block_state.map(BlockState));
                    true
                } else {
                    false
                }
            });

            if feature_blocks.len() > 0 {
                return Some(feature_blocks);
            }
        }

        return None;
    }
}
