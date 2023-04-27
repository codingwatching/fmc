use std::collections::HashMap;
use std::ops::{Index, IndexMut};

use crate::constants::*;
use crate::world::blocks::Blocks;
use fmc_networking::BlockId;

#[derive(PartialEq, Eq, Debug)]
pub enum ChunkType {
    /// The common chunk, all block positions are filled.
    Normal,
    /// Chunk has only been partially generated, meaning adjacent chunks have filled in some of the
    /// chunk's positions with blocks through feature generation.
    Partial,
    /// Chunk that only contains one type of block
    Uniform(BlockId),
}

// TODO: Is it necessary to pack the block state? It's sent to the clients so needs to be small.
// Maybe arbitrary data is wanted. When making, thought orientation obviously needed, and nice to
// change color of things like water or torch, but can't think of anything that is needed
// otherwise. XXX: It's used by the database to mark uniform chunks by setting it to u16::MAX.
pub struct Chunk {
    pub chunk_type: ChunkType,
    // TODO: Maybe store blocks as xzy for compression? Blocks are more likely to stay the same in
    // the y direction?
    /// Blocks are stored as one contiguous array. To access a block at the coordinate x,y,z
    /// (zero indexed) the formula x * CHUNK_SIZE^2 + y * CHUNK_SIZE + z is used.
    pub blocks: Vec<BlockId>,
    //block_entities: HashMap<IVec3, Entity>
    /// Block state containing optional information, see fmc_networking for bit layout
    pub block_state: HashMap<usize, u16>,
}

impl Chunk {
    pub fn new(block_id: BlockId) -> Self {
        let blocks = vec![block_id; CHUNK_SIZE.pow(3)];
        let block_state = HashMap::new();
        return Self {
            chunk_type: ChunkType::Normal,
            blocks,
            block_state,
        };
    }

    pub fn combine(&mut self, mut other: Chunk) {
        let air = Blocks::get().get_id("air");

        match self.chunk_type {
            ChunkType::Normal => {
                if other.chunk_type == ChunkType::Partial {
                    self.blocks
                        .iter_mut()
                        .zip(other.blocks)
                        .enumerate()
                        .for_each(|(position, (block, partial_block))| {
                            if *block == air && partial_block != air {
                                *block = partial_block;
                                if let Some(block_state) = other.block_state.remove(&position) {
                                    self.block_state.insert(position, block_state);
                                }
                            }
                        });
                }
            }
            ChunkType::Partial => {
                match other.chunk_type {
                    ChunkType::Normal => {
                        self.chunk_type = ChunkType::Normal;
                        self.blocks.iter_mut().zip(other.blocks).for_each(
                            |(block, other_block)| {
                                if other_block != air {
                                    *block = other_block;
                                }
                            },
                        );

                        for (position, state) in other.block_state.into_iter() {
                            self.block_state.insert(position, state);
                        }
                    }
                    ChunkType::Partial => {
                        self.blocks.iter_mut().zip(other.blocks).for_each(
                            |(block, other_block)| {
                                if *block == air {
                                    *block = other_block;
                                }
                            },
                        );

                        for (position, block_state) in other.block_state {
                            self.block_state.entry(position).or_insert(block_state);
                        }
                    }
                    ChunkType::Uniform(_) => unreachable!(),
                }
            }
            ChunkType::Uniform(block_id) => {
                self.chunk_type = ChunkType::Normal;
                self.blocks = vec![block_id; CHUNK_SIZE.pow(3)];
                self.combine(other);
            }
        }
    }
}

// So you can index like 'chunk[[1,2,3]]'
impl Index<[usize; 3]> for Chunk {
    type Output = BlockId;

    fn index(&self, idx: [usize; 3]) -> &Self::Output {
        return &self.blocks[idx[0] * CHUNK_SIZE.pow(2) + idx[1] * CHUNK_SIZE + idx[2]];
    }
}

impl IndexMut<[usize; 3]> for Chunk {
    fn index_mut(&mut self, idx: [usize; 3]) -> &mut Self::Output {
        return &mut self.blocks[idx[0] * CHUNK_SIZE.pow(2) + idx[1] * CHUNK_SIZE + idx[2]];
    }
}

impl Index<usize> for Chunk {
    type Output = BlockId;

    fn index(&self, idx: usize) -> &Self::Output {
        return &self.blocks[idx];
    }
}

impl IndexMut<usize> for Chunk {
    fn index_mut(&mut self, idx: usize) -> &mut Self::Output {
        return &mut self.blocks[idx];
    }
}
