use std::collections::HashMap;
use std::ops::{Index, IndexMut};
use std::slice::Iter;

use bevy::prelude::*;
use fmc_networking::BlockId;

use crate::constants::*;
use crate::utils;
use crate::world::blocks::BlockState;

#[derive(Component)]
pub struct ChunkMarker;

/// There are two kinds of chunks.
/// Uniform(air, solid stone, etc) chunks:
///     entity = None
///     blocks = Vec::with_capacity(1), contains type of block
/// Chunks with blocks:
///     entity = Some
///     blocks = Vec::with_capacity(CHUNK_SIZE^3)
#[derive(Clone)]
pub struct Chunk {
    // Entity in the ECS. Stores mesh, None if the chunk doesn't have one.
    pub entity: Option<Entity>,
    /// XXX: Notice that the coordinates align with the rendering world, the z axis extends
    /// out of the screen. 0,0,0 is the bottom left FAR corner. Not bottom left NEAR.
    /// A CHUNK_SIZE^3 array containing all the blocks in the chunk.
    /// Indexed by x*CHUNK_SIZE^2 + z*CHUNK_SIZE + y
    blocks: Vec<BlockId>,
    /// Optional block state
    pub block_state: HashMap<usize, BlockState>,
}

impl Chunk {
    /// Build a normal chunk
    pub fn new(
        entity: Entity,
        blocks: Vec<BlockId>,
        block_state: HashMap<usize, BlockState>,
    ) -> Self {
        return Self {
            entity: Some(entity),
            blocks,
            block_state,
        };
    }

    /// Create a new chunk of only air blocks; to be filled after creation.
    pub fn new_air(blocks: Vec<BlockId>, block_state: HashMap<usize, BlockState>) -> Self {
        assert!(blocks.len() == 1);

        return Self {
            entity: None,
            block_state,
            blocks,
        };
    }

    pub fn convert_uniform_to_full(&mut self) {
        if !self.is_uniform() {
            panic!("Tried to convert a non uniform chunk");
        }
        let block = self.blocks[0];
        self.blocks = vec![block; CHUNK_SIZE.pow(3)]
    }

    pub fn is_uniform(&self) -> bool {
        return self.blocks.len() == 1;
    }

    pub fn set_block_state(&mut self, block_index: usize, state: BlockState) {
        self.block_state.insert(block_index, state);
    }

    pub fn remove_block_state(&mut self, block_index: &usize) {
        self.block_state.remove(&block_index);
    }

    pub fn get_block_state(&self, x: usize, y: usize, z: usize) -> Option<BlockState> {
        let index = x << 8 | z << 4 | y;
        return self.block_state.get(&index).copied();
    }
}

impl Index<usize> for Chunk {
    type Output = BlockId;

    fn index(&self, idx: usize) -> &Self::Output {
        if self.is_uniform() {
            return &self.blocks[0];
        } else {
            return &self.blocks[idx];
        }
    }
}

impl IndexMut<usize> for Chunk {
    fn index_mut(&mut self, idx: usize) -> &mut Self::Output {
        if self.is_uniform() {
            return &mut self.blocks[0];
        } else {
            return &mut self.blocks[idx];
        }
    }
}

impl Index<[usize; 3]> for Chunk {
    type Output = BlockId;

    fn index(&self, idx: [usize; 3]) -> &Self::Output {
        if self.is_uniform() {
            return &self.blocks[0];
        } else {
            return &self.blocks[idx[0] * CHUNK_SIZE.pow(2) + idx[2] * CHUNK_SIZE + idx[1]];
        }
    }
}

impl IndexMut<[usize; 3]> for Chunk {
    fn index_mut(&mut self, idx: [usize; 3]) -> &mut Self::Output {
        if self.is_uniform() {
            return &mut self.blocks[0];
        } else {
            return &mut self.blocks[idx[0] * CHUNK_SIZE.pow(2) + idx[2] * CHUNK_SIZE + idx[1]];
        }
    }
}

impl Index<IVec3> for Chunk {
    type Output = BlockId;

    fn index(&self, idx: IVec3) -> &Self::Output {
        if self.is_uniform() {
            return &self.blocks[0];
        } else {
            let idx = utils::world_position_to_block_index(idx);
            return &self.blocks[idx];
        }
    }
}

impl IndexMut<IVec3> for Chunk {
    fn index_mut(&mut self, idx: IVec3) -> &mut Self::Output {
        if self.is_uniform() {
            return &mut self.blocks[0];
        } else {
            let idx = utils::world_position_to_block_index(idx);
            return &mut self.blocks[idx];
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub enum ChunkFace {
    // Forward is +z direction
    Top,
    Bottom,
    Right,
    Left,
    Front,
    Back,
    None,
}

impl ChunkFace {
    // TODO: Is it better to use an associated constant for the normals?
    pub fn normal(&self) -> Vec3 {
        match self {
            &ChunkFace::Front => Vec3::Z,
            &ChunkFace::Back => -Vec3::Z,
            &ChunkFace::Right => Vec3::X,
            &ChunkFace::Left => -Vec3::X,
            &ChunkFace::Top => Vec3::Y,
            &ChunkFace::Bottom => -Vec3::Y,
            &ChunkFace::None => panic!("Can't get normal of ChunkFace::None"),
        }
    }

    pub fn opposite(&self) -> Self {
        match self {
            &ChunkFace::Front => ChunkFace::Back,
            &ChunkFace::Back => ChunkFace::Front,
            &ChunkFace::Right => ChunkFace::Left,
            &ChunkFace::Left => ChunkFace::Right,
            &ChunkFace::Top => ChunkFace::Bottom,
            &ChunkFace::Bottom => ChunkFace::Top,
            &ChunkFace::None => panic!("Can't get opposite of ChunkFace::None"),
        }
    }

    pub fn is_opposite(&self, check_opposing: &Self) -> bool {
        match self {
            &ChunkFace::Front => check_opposing == &ChunkFace::Back,
            &ChunkFace::Back => check_opposing == &ChunkFace::Front,
            &ChunkFace::Right => check_opposing == &ChunkFace::Left,
            &ChunkFace::Left => check_opposing == &ChunkFace::Right,
            &ChunkFace::Top => check_opposing == &ChunkFace::Bottom,
            &ChunkFace::Bottom => check_opposing == &ChunkFace::Top,
            &ChunkFace::None => panic!("Can't get opposite of ChunkFace::None"),
        }
    }

    /// Finds the chunk faces orthogonal to self.
    pub fn surrounding(&self) -> Vec<Self> {
        let mut all = vec![
            Self::Front,
            Self::Back,
            Self::Left,
            Self::Right,
            Self::Top,
            Self::Bottom,
        ];
        all.retain(|dir| dir != &self.opposite() && dir != self);
        return all;
    }

    /// Find the chunk faces that are orthogonal on each ChunkFace in 'surrounding', but that do not
    /// go in the direction of self, or its opposite.
    pub fn orthogonal(&self, surrounding: &Vec<Self>) -> Vec<[Self; 2]> {
        let mut ortho = Vec::with_capacity(4);
        for chunk_face in surrounding {
            if self == &ChunkFace::Top || self == &ChunkFace::Bottom {
                match chunk_face {
                    ChunkFace::Right => ortho.push([ChunkFace::Front, ChunkFace::Back]),
                    ChunkFace::Left => ortho.push([ChunkFace::Front, ChunkFace::Back]),
                    ChunkFace::Front => ortho.push([ChunkFace::Left, ChunkFace::Right]),
                    ChunkFace::Back => ortho.push([ChunkFace::Left, ChunkFace::Right]),
                    _ => {}
                }
            } else if self == &ChunkFace::Front || self == &ChunkFace::Back {
                match chunk_face {
                    ChunkFace::Right => ortho.push([ChunkFace::Top, ChunkFace::Bottom]),
                    ChunkFace::Left => ortho.push([ChunkFace::Top, ChunkFace::Bottom]),
                    ChunkFace::Top => ortho.push([ChunkFace::Left, ChunkFace::Right]),
                    ChunkFace::Bottom => ortho.push([ChunkFace::Left, ChunkFace::Right]),
                    _ => {}
                }
            } else if self == &ChunkFace::Right || self == &ChunkFace::Left {
                match chunk_face {
                    ChunkFace::Front => ortho.push([ChunkFace::Top, ChunkFace::Bottom]),
                    ChunkFace::Back => ortho.push([ChunkFace::Top, ChunkFace::Bottom]),
                    ChunkFace::Top => ortho.push([ChunkFace::Front, ChunkFace::Back]),
                    ChunkFace::Bottom => ortho.push([ChunkFace::Front, ChunkFace::Back]),
                    _ => {}
                }
            }
        }
        return ortho;
    }

    /// Moves the position a chunk's length in the direction of the face.
    pub fn shift_position(&self, mut position: IVec3) -> IVec3 {
        match self {
            ChunkFace::Front => position.z += CHUNK_SIZE as i32,
            ChunkFace::Back => position.z -= CHUNK_SIZE as i32,
            ChunkFace::Right => position.x += CHUNK_SIZE as i32,
            ChunkFace::Left => position.x -= CHUNK_SIZE as i32,
            ChunkFace::Top => position.y += CHUNK_SIZE as i32,
            ChunkFace::Bottom => position.y -= CHUNK_SIZE as i32,
            ChunkFace::None => {}
        }
        return position;
    }

    /// Returns the chunk face the vector placed in the middle of the chunk points at.
    pub fn convert_vector(vec: &Vec3) -> Self {
        let abs = vec.abs();
        if abs.x > abs.y && abs.x > abs.z {
            if vec.x < 0.0 {
                return ChunkFace::Left;
            } else {
                return ChunkFace::Right;
            }
        } else if abs.y > abs.x && abs.y > abs.z {
            if vec.y < 0.0 {
                return ChunkFace::Bottom;
            } else {
                return ChunkFace::Top;
            }
        } else {
            if vec.z < 0.0 {
                return ChunkFace::Back;
            } else {
                return ChunkFace::Front;
            }
        }
    }

    /// Given a relative block position that is immediately adjacent to one of the chunk's faces, return the face.
    pub fn from_position(pos: &IVec3) -> Self {
        if pos.z > (CHUNK_SIZE - 1) as i32 {
            return ChunkFace::Front;
        } else if pos.z < 0 {
            return ChunkFace::Back;
        } else if pos.x > (CHUNK_SIZE - 1) as i32 {
            return ChunkFace::Right;
        } else if pos.x < 0 {
            return ChunkFace::Left;
        } else if pos.y > (CHUNK_SIZE - 1) as i32 {
            return ChunkFace::Top;
        } else if pos.y < 0 {
            return ChunkFace::Bottom;
        } else {
            return ChunkFace::None;
        }
    }

    pub fn iter() -> Iter<'static, ChunkFace> {
        use self::ChunkFace::*;
        static CHUNK_FACES: [ChunkFace; 6] = [Front, Back, Right, Left, Top, Bottom];
        CHUNK_FACES.iter()
    }
}
