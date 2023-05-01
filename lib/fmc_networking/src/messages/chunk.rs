use std::collections::HashMap;

use crate::BlockId;
use bevy::prelude::*;
use fmc_networking_derive::{ClientBound, NetworkMessage, ServerBound};
use serde::{Deserialize, Serialize};

/// Asks the server for a set of chunks.
#[derive(NetworkMessage, ServerBound, Serialize, Deserialize, Debug)]
pub struct ChunkRequest {
    /// Position of the chunks the client wants
    pub chunks: Vec<IVec3>,
}

impl ChunkRequest {
    /// Blank response
    pub fn new() -> Self {
        Self { chunks: Vec::new() }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Chunk {
    pub position: IVec3,
    // Blocks are stored as one array. To access a block at the coordinate x,y,z
    // (zero indexed) the formula x * CHUNK_SIZE^2 + y * CHUNK_SIZE + z is used.
    pub blocks: Vec<BlockId>,
    // Packed u16 containing optional info.
    // bits:
    //     0000 0000 0000 unused
    //     0000
    //       ^^-north/south/east/west
    //      ^---centered
    //     ^----upside down
    pub block_state: HashMap<usize, u16>,
}

/// Reponse to a ChunkRequest.
/// Contains a set of chunks.
#[derive(NetworkMessage, ClientBound, Serialize, Deserialize, Debug, Clone)]
pub struct ChunkResponse {
    /// The chunks the client requested.
    pub chunks: Vec<Chunk>,
}

impl ChunkResponse {
    /// new empty response
    pub fn new() -> Self {
        Self { chunks: Vec::new() }
    }

    pub fn add_chunk(
        &mut self,
        position: IVec3,
        blocks: Vec<BlockId>,
        block_state: HashMap<usize, u16>,
    ) {
        self.chunks.push(Chunk {
            position,
            blocks,
            block_state,
        })
    }
}

/// Clients need to send this when it drops chunks.
#[derive(NetworkMessage, ServerBound, Serialize, Deserialize, Debug)]
pub struct UnsubscribeFromChunks {
    /// Position of the chunks the client wants
    pub chunks: Vec<IVec3>,
}
