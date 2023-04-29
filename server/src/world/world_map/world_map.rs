// The chunk map keeps track of where chunk entities are in space.
// Whenever you need access to a chunk's data through its position you need to do a double lookup,
// one in the chunk map to find the entity, and one in an ECS query. This is inefficient for
// frequent lookups, so some data is also stored directly in the chunk map, i.e block data.

use bevy::{math::DVec3, prelude::*};

use crate::{
    bevy_extensions::f64_transform::F64Transform,
    utils,
    world::{
        blocks::{BlockFace, Blocks, Friction},
        world_map::chunk::{Chunk, ChunkType},
    },
};
use fmc_networking::BlockId;

#[derive(Default, Resource)]
pub struct WorldMap {
    pub chunks: std::collections::HashMap<IVec3, Chunk>,
}

impl WorldMap {
    pub fn contains_chunk(&self, pos: &IVec3) -> bool {
        return self.chunks.contains_key(pos);
    }

    pub fn get_chunk(&self, pos: &IVec3) -> Option<&Chunk> {
        return self.chunks.get(&pos);
    }

    pub fn get_chunk_mut(&mut self, pos: &IVec3) -> Option<&mut Chunk> {
        return self.chunks.get_mut(&pos);
    }

    pub fn insert(&mut self, pos: IVec3, value: Chunk) {
        self.chunks.insert(pos, value);
    }

    pub fn get_block(&self, position: IVec3) -> Option<BlockId> {
        let (chunk_pos, index) = utils::world_position_to_chunk_position_and_block_index(position);

        if let Some(chunk) = self.get_chunk(&chunk_pos) {
            match chunk.chunk_type {
                ChunkType::Normal => Some(chunk[index]),
                ChunkType::Partial => None,
                ChunkType::Uniform(block_id) => Some(block_id),
            }
        } else {
            return None;
        }
    }

    pub fn get_block_state(&self, position: IVec3) -> Option<u16> {
        let (chunk_pos, index) = utils::world_position_to_chunk_position_and_block_index(position);

        if let Some(chunk) = self.get_chunk(&chunk_pos) {
            return chunk.block_state.get(&index).copied();
        } else {
            return None;
        }
    }

    /// Find which block the transform is looking at, if any.
    pub fn raycast_to_block(
        &self,
        transform: &F64Transform,
        distance: f64,
    ) -> Option<(IVec3, BlockId, BlockFace)> {
        let blocks = Blocks::get();
        let forward = transform.forward();
        let direction = forward.signum();

        // How far along the forward vector you need to go to hit the next block in each direction.
        // This makes more sense if you mentally align it with the block grid.
        //
        // This relies on some peculiar behaviour where normally f32.fract() would retain the
        // sign of the number, Vec3.fract() instead does self - self.floor(). This results in
        // having the correct value for the negative direction, but it has to be flipped for the
        // positive direction, which is the vec3::select.
        let mut distance_next = transform.translation.fract();
        distance_next = DVec3::select(
            direction.cmpeq(DVec3::ONE),
            1.0 - distance_next,
            distance_next,
        );
        distance_next = distance_next / forward.abs();

        // How far along the forward vector you need to go to traverse one block in each direction.
        let t_block = 1.0 / forward.abs();
        // +/-1 to shift block_pos when it hits the grid
        let step = direction.as_ivec3();

        // The origin block of the ray.
        // From this point we can jump from one block to another easily.
        let mut block_pos = transform.translation.floor().as_ivec3();

        while (distance_next.min_element() * forward).length_squared() < distance.powi(2) {
            if distance_next.x < distance_next.y && distance_next.x < distance_next.z {
                block_pos.x += step.x;
                distance_next.x += t_block.x;
                // Have to do this for each branch, so it get's a little noisy.
                if let Some(block_id) = self.get_block(block_pos) {
                    if let Friction::Drag(_) = &blocks.get_config(&block_id).friction {
                        continue;
                    }

                    let block_side = if direction.x == 1.0 {
                        BlockFace::Left
                    } else {
                        BlockFace::Right
                    };

                    return Some((block_pos, block_id, block_side));
                }
            } else if distance_next.z < distance_next.x && distance_next.z < distance_next.y {
                block_pos.z += step.z;
                distance_next.z += t_block.z;

                if let Some(block_id) = self.get_block(block_pos) {
                    if let Friction::Drag(_) = &blocks.get_config(&block_id).friction {
                        continue;
                    }

                    let block_side = if direction.z == 1.0 {
                        BlockFace::Back
                    } else {
                        BlockFace::Front
                    };
                    return Some((block_pos, block_id, block_side));
                }
            } else {
                block_pos.y += step.y;
                distance_next.y += t_block.y;

                if let Some(block_id) = self.get_block(block_pos) {
                    if let Friction::Drag(_) = &blocks.get_config(&block_id).friction {
                        continue;
                    }

                    let block_side = if direction.y == 1.0 {
                        BlockFace::Bottom
                    } else {
                        BlockFace::Top
                    };

                    return Some((block_pos, block_id, block_side));
                }
            }
        }
        return None;
    }
}
