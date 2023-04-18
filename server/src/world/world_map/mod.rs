use std::collections::HashMap;

use bevy::prelude::*;

use fmc_networking::{messages, BlockId, NetworkServer};

pub mod chunk;
pub mod chunk_manager;
pub mod terrain_generation;
mod world_map;

pub use world_map::WorldMap;

use crate::utils;

use self::{
    chunk::{Chunk, ChunkType},
    chunk_manager::ChunkSubscriptions,
};

pub struct WorldMapPlugin;
impl Plugin for WorldMapPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugin(chunk_manager::ChunkManagerPlugin)
            .add_plugin(terrain_generation::TerrainGenerationPlugin)
            .add_event::<BlockUpdate>()
            .add_event::<ChangedBlockEvent>()
            .add_systems(PreUpdate, handle_block_updates);
    }
}

// TODO: Unfortunate naming, a BlockUpdate happens, and this is the response that is sent out
// widely, but they kinda sound like the same thing.
//
// Some types of block need to know whenever a block adjacent to it changes (for example water
// needs to know when it should spread), instead of sending out the position of the changed block,
// this struct is constructed to save on lookup time as each system that reacts to this would need
// to query all the adjacent block positions individually.
//
/// Event sent in response to a block update.
pub struct ChangedBlockEvent {
    pub position: IVec3,
    pub from: BlockId,
    pub to: BlockId,
    pub top: BlockId,
    pub bottom: BlockId,
    pub right: BlockId,
    pub left: BlockId,
    pub front: BlockId,
    pub back: BlockId,
}

impl ChangedBlockEvent {
    pub fn has_adjacent_block(&self, block: BlockId) -> Option<(IVec3, BlockId)> {
        if self.top == block {
            return Some((self.position + IVec3::new(0, 1, 0), self.top));
        } else if self.bottom == block {
            return Some((self.position + IVec3::new(0, -1, 0), self.top));
        } else if self.right == block {
            return Some((self.position + IVec3::new(1, 0, 0), self.top));
        } else if self.left == block {
            return Some((self.position + IVec3::new(-1, 0, 0), self.top));
        } else if self.front == block {
            return Some((self.position + IVec3::new(0, 0, 1), self.top));
        } else if self.back == block {
            return Some((self.position + IVec3::new(0, 0, -1), self.top));
        } else {
            return None;
        }
    }
}

// TODO: Don't know where to put this yet.
pub enum BlockUpdate {
    /// Change one block to another. Fields are position/block id/block state
    Change(IVec3, BlockId, Option<u16>),
    ///// Change the state of a block.
    //State(IVec3, u8),
    // Particles?
}

// TODO: Batch block updates into their corresponding chunks so they can be applied together
// avoiding lookups.
// Applies block updates to the world and sends them to the players.
fn handle_block_updates(
    mut world_map: ResMut<world_map::WorldMap>,
    mut block_events: EventReader<BlockUpdate>,
    chunk_subsriptions: Res<ChunkSubscriptions>,
    net: Res<NetworkServer>,
) {
    for event in block_events.iter() {
        match event {
            BlockUpdate::Change(position, block_id, block_state) => {
                let (chunk_pos, block_index) =
                    utils::world_position_to_chunk_position_and_block_index(*position);

                let chunk = if let Some(c) = world_map.get_chunk_mut(&chunk_pos) {
                    c
                } else {
                    panic!("Tried to change block in non-existing chunk");
                };

                match chunk.chunk_type {
                    ChunkType::Uniform(uniform_block_id) => {
                        let mut new_chunk = Chunk::new(uniform_block_id);

                        new_chunk[block_index] = *block_id;
                        if let Some(state) = block_state {
                            new_chunk.block_state.insert(block_index, *state);
                        }

                        world_map.insert(chunk_pos, new_chunk);
                    }
                    ChunkType::Normal => {
                        chunk[block_index] = *block_id;
                        if let Some(state) = block_state {
                            chunk.block_state.insert(block_index, *state);
                        }
                    }
                    ChunkType::Partial => unreachable!(),
                }

                if let Some(subscribers) = chunk_subsriptions.get_subscribers(&chunk_pos) {
                    net.send_many(
                        subscribers,
                        // TODO: All updates in the same chunk should be collected and sent
                        // together.
                        messages::BlockUpdates {
                            chunk_position: chunk_pos,
                            blocks: vec![(block_index, *block_id)],
                            block_state: match *block_state {
                                Some(s) => HashMap::from([(block_index, s)]),
                                None => HashMap::new(),
                            },
                        },
                    );
                }
            }
        }
    }
}

// Every block change is immediately saved to disc.
//fn save_block_updates_to_database(
//    database: Res<Arc<Database>>,
//    block_events: EventReader<BlockEvent>,
//) {
//    let chunk_block_updates: HashMap<Ivec3, Vec<(IVec3, BlockId)>> = HashMap::new();
//    //let chunk_state_updates: HashMap<Ivec3, (IVec3, State?)> = HashMap::new();
//    for event in block_events.get_reader() {
//        match event {
//            BlockUpdate::Change(pos, id) => {
//                let (chunk_pos, index) = utils::world_coord_to_chunk_coord_and_block_index(pos);
//                chunk_block_updates
//                    .entry(&chunk_pos)
//                    .or_insert(Vec::new())
//                    .push((index, id))
//            }
//            _ => {}
//        }
//    }
//    if !chunk_block_updates.is_empty() {
//        let connection = database.get_connection();
//        for (chunk_pos, blocks) in chunk_block_updates.iter() {
//            database::
//        }
//    }
//}
