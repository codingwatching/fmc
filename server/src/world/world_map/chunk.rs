use std::collections::HashMap;
use std::ops::{Index, IndexMut};
use std::sync::Arc;

use crate::database::Database;
use crate::world::blocks::{BlockState, Blocks};
use crate::{constants::*, utils};
use bevy::prelude::IVec3;
use fmc_networking::BlockId;

use super::terrain_generation::TerrainGenerator;

#[derive(PartialEq, Eq, Debug)]
pub enum ChunkStatus {
    // Fully generated, populated by different block types
    Finished,
    // A chunk that is either waiting for its neighbors to finish, or was generated only as a
    // neighbor, to finish another.
    Unfinished {
        // Blocks that have been saved to the database.
        saved_blocks: HashMap<usize, (BlockId, Option<u16>)>,
        // TODO: It does 26x2 lookups to populate its own neighbors and the ones for its
        // partial chunks. To speed up make it vectors. The positional offset of the chunks are really
        // unneeded other than for knowing which chunk is the one immediately below, as that one
        // takes priority. Simply insert it at the start of the vec and push the rest. Iterate
        // through normally and it will take precedent.
        //
        // Blocks from neighboring chunks
        neighbors: HashMap<IVec3, HashMap<usize, (BlockId, Option<u16>)>>,
    },
}

impl ChunkStatus {
    #[track_caller]
    pub fn neighbors(
        &mut self,
    ) -> Option<&mut HashMap<IVec3, HashMap<usize, (BlockId, Option<u16>)>>> {
        match self {
            Self::Unfinished {
                neighbors: partial_chunks,
                ..
            } => Some(partial_chunks),
            Self::Finished => None,
        }
    }

    pub fn unwrap_saved_blocks(&mut self) -> &mut HashMap<usize, (BlockId, Option<u16>)> {
        match self {
            Self::Unfinished { saved_blocks, .. } => saved_blocks,
            _ => panic!("Called 'Chunk::unwrap_saved_blocks()' on a finished chunk"),
        }
    }
}
// TODO: Is it necessary to pack the block state? It's sent to the clients so needs to be small.
// Maybe arbitrary data is wanted. When making, thought orientation obviously needed, and nice to
// change color of things like water or torch, but can't think of anything that is needed
// otherwise. XXX: It's used by the database to mark uniform chunks by setting it to
// u16::MAX(an invalid state).
pub struct Chunk {
    pub status: ChunkStatus,
    // The blocks that were generated from this chunk that stretched into other chunks.
    pub partial_chunks: HashMap<IVec3, HashMap<usize, (BlockId, Option<u16>)>>,
    // Blocks are stored as one contiguous array. To access a block at the coordinate x,y,z
    // (zero indexed) the formula x * CHUNK_SIZE^2 + z * CHUNK_SIZE + y is used.
    pub blocks: Vec<BlockId>,
    // Block state containing optional information, see `BlockState` for bit layout
    pub block_state: HashMap<usize, u16>,
}

impl Chunk {
    pub fn new_regular(block_id: BlockId) -> Self {
        return Self {
            status: ChunkStatus::Unfinished {
                saved_blocks: HashMap::new(),
                neighbors: HashMap::new(),
            },
            partial_chunks: HashMap::new(),
            blocks: vec![block_id; CHUNK_SIZE.pow(3)],
            block_state: HashMap::new(),
        };
    }

    pub fn new_uniform(block_id: BlockId) -> Self {
        return Self {
            status: ChunkStatus::Unfinished {
                saved_blocks: HashMap::new(),
                neighbors: HashMap::new(),
            },
            partial_chunks: HashMap::new(),
            blocks: vec![block_id; 1],
            block_state: HashMap::new(),
        };
    }

    pub fn get_block_state(&self, index: &usize) -> Option<BlockState> {
        return self.block_state.get(index).copied().map(BlockState);
    }

    pub fn is_uniform(&self) -> bool {
        return self.blocks.len() == 1;
    }

    pub fn try_convert_uniform_to_regular(&mut self) {
        if !self.is_uniform() {
            return;
        }
        let block_id = self.blocks[0];
        self.blocks = vec![block_id; CHUNK_SIZE.pow(3)];
    }

    // Load/Generate a chunk
    pub async fn load(
        position: IVec3,
        terrain_generator: Arc<TerrainGenerator>,
        database: Arc<Database>,
    ) -> (IVec3, Chunk) {
        let mut partial_chunks: HashMap<IVec3, HashMap<usize, (BlockId, Option<u16>)>> =
            HashMap::new();

        for x in -1..=1 {
            for y in -1..=1 {
                for z in -1..=1 {
                    let pos = IVec3::new(x, y, z) * CHUNK_SIZE as i32;
                    if pos != IVec3::ZERO {
                        partial_chunks.insert(pos, HashMap::new());
                    }
                }
            }
        }

        let air = Blocks::get().get_id("air");

        let saved_blocks = database.load_chunk_blocks(&position).await;

        let (uniform, blocks) = terrain_generator.generate_chunk(position).await;

        if uniform && saved_blocks.len() == 0 {
            let block = *blocks.get(&position).unwrap();
            let mut chunk = Chunk::new_uniform(block);
            chunk.partial_chunks = partial_chunks;

            return (position, chunk);
        }

        let mut chunk = Chunk::new_regular(air);
        chunk.partial_chunks = partial_chunks;

        for (world_pos, block) in blocks {
            let (chunk_pos, block_index) =
                utils::world_position_to_chunk_position_and_block_index(world_pos);
            let chunk_offset = chunk_pos - position;

            if chunk_offset == IVec3::ZERO {
                if block == air {
                    continue;
                }
                chunk[block_index] = block;
            } else {
                chunk
                    .partial_chunks
                    .get_mut(&chunk_offset)
                    .expect(&format!("{}", chunk_offset))
                    .insert(block_index, (block, None));
            }
        }

        *chunk.status.unwrap_saved_blocks() = saved_blocks;

        return (position, chunk);
    }

    pub fn set_block_state(&mut self, block_index: usize, block_state: Option<BlockState>) {
        if let Some(block_state) = block_state {
            self.block_state.insert(block_index, block_state.0);
        } else {
            self.block_state.remove(&block_index);
        }
    }

    pub fn try_finish(&mut self) -> bool {
        let neighbors = match self.status.neighbors() {
            Some(n) => n,
            None => return false,
        };

        if !(neighbors.len() == 26) {
            // The chunk is ready when all 26 neighbors have had a chance to generate blocks into
            // it.
            return false;
        }

        let status = std::mem::replace(&mut self.status, ChunkStatus::Finished);

        let ChunkStatus::Unfinished {
            saved_blocks,
            mut neighbors,
        } = status
        else {
            unreachable!()
        };
        let below_blocks = neighbors.remove(&IVec3::new(0, -16, 0)).unwrap();

        let air = Blocks::get().get_id("air");
        for (_, blocks) in [(IVec3::new(0, -16, 0), below_blocks)]
            .into_iter()
            .chain(neighbors.drain())
        {
            if blocks.len() > 0 && self.is_uniform() {
                self.try_convert_uniform_to_regular();
            }

            for (block_index, (block, block_state)) in blocks.into_iter() {
                if &self[block_index] == &air {
                    self[block_index] = block;
                    if let Some(block_state) = block_state {
                        self.set_block_state(block_index, Some(BlockState(block_state)));
                    }
                }
            }
        }

        for (block_index, (block, block_state)) in saved_blocks {
            self[block_index] = block;
            if let Some(block_state) = block_state {
                self.set_block_state(block_index, Some(BlockState(block_state)));
            }
        }

        return true;
    }
}

// So you can index like 'chunk[[1,2,3]]'
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
            // TODO: Probably convert chunk
            panic!();
        } else {
            return &mut self.blocks[idx[0] * CHUNK_SIZE.pow(2) + idx[2] * CHUNK_SIZE + idx[1]];
        }
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
            // TODO: Probably convert chunk
            panic!();
        } else {
            return &mut self.blocks[idx];
        }
    }
}
