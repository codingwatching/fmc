use std::{
    collections::{HashMap, VecDeque},
    ops::Index,
};

use bevy::prelude::*;
use fmc_networking::{messages, NetworkData};

use crate::{
    constants::CHUNK_SIZE,
    game_state::GameState,
    utils,
    world::{
        blocks::Blocks,
        world_map::{ChunkFace, WorldMap},
        Origin,
    },
};

use super::chunk::{ChunkMeshEvent, ExpandedLightChunk};

// TODO: There's still lag spikes from time to time. I think it is caused by some cascading
// behaviour when handling Remove updates. I works fine without it(I think), and I have already
// fixed a similar issue. Just measure if there are excessively many lighting updates and which
// type they are.
// TODO: When removing a block I noticed once the light value wasn't updated, intermittent. 

pub struct LightingPlugin;
impl Plugin for LightingPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(LightMap::default())
            .add_event::<TestFinishedLightingEvent>()
            .insert_resource(LightUpdateQueue::default())
            .add_systems(
                Update,
                (
                    queue_light_updates,
                    add_light_chunks,
                    propagate_light,
                    send_chunk_mesh_events,
                    light_chunk_unloading.run_if(resource_changed::<Origin>()),
                )
                    .run_if(in_state(GameState::Playing)),
            );
    }
}

#[derive(Resource, Default)]
pub struct LightMap {
    chunks: HashMap<IVec3, LightChunk>,
}

impl LightMap {
    pub fn get_light(&self, block_position: IVec3) -> Option<Light> {
        let (chunk_pos, block_index) = utils::world_position_to_chunk_position_and_block_index(block_position);
        if let Some(light_chunk) = self.chunks.get(&chunk_pos) {
            Some(light_chunk[block_index])
        } else {
            None
        }
    }
    pub fn get_expanded_chunk(&self, position: IVec3) -> Option<ExpandedLightChunk> {
        let center_chunk = match self.chunks.get(&position) {
            Some(t) => t.clone(),
            None => {
                return None;
            }
        };

        let top_position = position + IVec3::new(0, CHUNK_SIZE as i32, 0);
        let top_chunk = match self.chunks.get(&top_position) {
            Some(t) => t,
            None => {
                return None;
            }
        };

        let bottom_position = position - IVec3::new(0, CHUNK_SIZE as i32, 0);
        let bottom_chunk = match self.chunks.get(&bottom_position) {
            Some(t) => t,
            None => {
                return None;
            }
        };

        let right_position = position + IVec3::new(CHUNK_SIZE as i32, 0, 0);
        let right_chunk = match self.chunks.get(&right_position) {
            Some(t) => t,
            None => {
                return None;
            }
        };

        let left_position = position - IVec3::new(CHUNK_SIZE as i32, 0, 0);
        let left_chunk = match self.chunks.get(&left_position) {
            Some(t) => t,
            None => {
                return None;
            }
        };

        let front_position = position + IVec3::new(0, 0, CHUNK_SIZE as i32);
        let front_chunk = match self.chunks.get(&front_position) {
            Some(t) => t,
            None => {
                return None;
            }
        };

        let back_position = position - IVec3::new(0, 0, CHUNK_SIZE as i32);
        let back_chunk = match self.chunks.get(&back_position) {
            Some(t) => t,
            None => {
                return None;
            }
        };

        let center = center_chunk;
        let mut top: [[Light; CHUNK_SIZE]; CHUNK_SIZE] = Default::default();
        let mut bottom: [[Light; CHUNK_SIZE]; CHUNK_SIZE] = Default::default();
        let mut right: [[Light; CHUNK_SIZE]; CHUNK_SIZE] = Default::default();
        let mut left: [[Light; CHUNK_SIZE]; CHUNK_SIZE] = Default::default();
        let mut front: [[Light; CHUNK_SIZE]; CHUNK_SIZE] = Default::default();
        let mut back: [[Light; CHUNK_SIZE]; CHUNK_SIZE] = Default::default();

        for i in 0..CHUNK_SIZE {
            for j in 0..CHUNK_SIZE {
                top[i][j] = top_chunk[[i, 0, j]];
                bottom[i][j] = bottom_chunk[[i, CHUNK_SIZE - 1, j]];
                right[i][j] = right_chunk[[0, i, j]];
                left[i][j] = left_chunk[[CHUNK_SIZE - 1, i, j]];
                back[i][j] = back_chunk[[i, j, CHUNK_SIZE - 1]];
                front[i][j] = front_chunk[[i, j, 0]];
            }
        }

        return Some(ExpandedLightChunk {
            center,
            top,
            bottom,
            right,
            left,
            front,
            back,
        });
    }
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Light(pub u8);

impl Light {
    const SUNLIGHT_MASK: u8 = 0b1111_0000;
    const ARTIFICIAL_MASK: u8 = 0b0000_1111;

    fn new(sunlight: u8, artificial: u8) -> Self {
        return Self(sunlight << 4 | artificial);
    }

    fn is_sunlight(&self) -> bool {
        return self.0 & Self::SUNLIGHT_MASK != 0;
    }

    pub fn sunlight(&self) -> u8 {
        return self.0 >> 4;
    }

    pub fn set_sunlight(&mut self, light: u8) {
        self.0 = self.0 & Self::ARTIFICIAL_MASK | (light << 4);
    }

    pub fn artificial(&self) -> u8 {
        return self.0 & Self::ARTIFICIAL_MASK;
    }

    pub fn set_artificial(&mut self, light: u8) {
        self.0 = self.0 & Self::SUNLIGHT_MASK | light;
    }

    fn can_propagate(&self) -> bool {
        return self.sunlight() > 1 || self.artificial() > 1;
    }

    fn decrement(self) -> Self {
        return self.decrement_sun(1).decrement_artificial();
    }

    fn decrement_artificial(self) -> Self {
        if self.0 & Self::ARTIFICIAL_MASK != 0 {
            return Self(self.0 - 1);
        } else {
            return self;
        }
    }

    fn decrement_sun(self, decrement: u8) -> Self {
        let sunlight = (self.0 >> 4).saturating_sub(decrement);
        return Light((self.0 & Self::ARTIFICIAL_MASK) | (sunlight << 4));
    }
}

// Light from blocks and the sky are combined into one u8, 4 bits each, max 16 light levels per.
#[derive(Clone, Debug)]
pub enum LightChunk {
    // Uniform lighting chunks are used to save space, and are only used for sunlight. 15 when
    // transparent, and 0 when opaque.
    Uniform(Light),
    Normal(Vec<Light>),
}

impl Index<[usize; 3]> for LightChunk {
    type Output = Light;

    fn index(&self, idx: [usize; 3]) -> &Self::Output {
        match self {
            Self::Uniform(light) => light,
            Self::Normal(lights) => &lights[idx[0] << 8 | idx[2] << 4 | idx[1]],
        }
    }
}

impl Index<usize> for LightChunk {
    type Output = Light;

    fn index(&self, idx: usize) -> &Self::Output {
        match self {
            Self::Uniform(light) => light,
            Self::Normal(lights) => &lights[idx],
        }
    }
}

struct LightPropagationUpdate {
    // Index in the chunk of the block the update is to be applied to.
    index: usize,
    // Light value of the block this update originated from, and which 'way' it should propagate.
    propagation: PropagationType,
}

#[derive(Clone, Copy, Debug)]
enum PropagationType {
    Add(Light),
    Remove(Light),
}

#[derive(Resource, Default)]
struct LightUpdateQueue {
    // To cut down on lighting calculations from sunlight it uses a ring buffer. When light
    // propagates down it is placed at the back of the queue, and when in any other direction at
    // the front. When it then does pop_back, it will always pick the sunlight with the highest
    // priority.
    chunks: HashMap<IVec3, VecDeque<LightPropagationUpdate>>,
}

// Queues light updates when blocks change.
fn queue_light_updates(
    mut light_updates: ResMut<LightUpdateQueue>,
    mut block_updates: EventReader<NetworkData<messages::BlockUpdates>>,
) {
    let blocks = Blocks::get();

    for block_update in block_updates.iter() {
        for (block_index, block_id) in block_update.blocks.iter() {
            // TODO: Respect blocks that emit light.
            let _block_config = match blocks.get_config(block_id) {
                Some(config) => config,
                None => continue,
            };

            if *block_index > CHUNK_SIZE.pow(3) {
                continue;
            }

            light_updates
                .chunks
                .entry(block_update.chunk_position)
                .or_insert(VecDeque::with_capacity(CHUNK_SIZE.pow(3)))
                .push_back(LightPropagationUpdate {
                    index: *block_index,
                    propagation: PropagationType::Remove(Light::new(0, 0)),
                });
        }
    }
}

fn add_light_chunks(
    mut light_map: ResMut<LightMap>,
    mut light_updates: ResMut<LightUpdateQueue>,
    mut chunk_responses: EventReader<NetworkData<messages::ChunkResponse>>,
) {
    let blocks = Blocks::get();
    for response in chunk_responses.iter() {
        for chunk in response.chunks.iter() {
            let mut new_chunk = if chunk.blocks.len() == 1 {
                let block_config = match blocks.get_config(&chunk.blocks[0]) {
                    Some(c) => c,
                    None => continue,
                };
                if block_config.is_transparent() && block_config.light_attenuation() == 0 {
                    LightChunk::Uniform(Light::new(15, 0))
                } else {
                    LightChunk::Uniform(Light::new(0, 0))
                }
            } else {
                LightChunk::Normal(vec![Light::new(0, 0); CHUNK_SIZE.pow(3)])
            };

            if let Some(above_light_chunk) = light_map
                .chunks
                .get(&ChunkFace::Top.offset_position(chunk.position))
            {
                if !matches!(above_light_chunk, LightChunk::Uniform(light) if light.sunlight() == 15)
                    && matches!(new_chunk, LightChunk::Uniform(_))
                {
                    // If the above chunk is uniform light level 0 or a normal light chunk, this new
                    // chunk below should be light level 0 too.
                    new_chunk = LightChunk::Uniform(Light::new(0, 0));
                }

                if !matches!(above_light_chunk, LightChunk::Uniform(light) if light.sunlight() == 0)
                {
                    let chunk_light_updates = light_updates
                        .chunks
                        .entry(chunk.position)
                        .or_insert(VecDeque::with_capacity(CHUNK_SIZE.pow(3)));
                    for x in 0..CHUNK_SIZE {
                        for z in 0..CHUNK_SIZE {
                            chunk_light_updates.push_back(LightPropagationUpdate {
                                index: x << 8 | z << 4 | 15,
                                propagation: PropagationType::Add(match above_light_chunk {
                                    LightChunk::Uniform(light) => *light,
                                    LightChunk::Normal(light) => {
                                        if light[x << 8 | z << 4].sunlight() == 15 {
                                            light[x << 8 | z << 4]
                                        } else {
                                            continue;
                                        }
                                    }
                                }),
                            });
                        }
                    }
                }
            }

            if let LightChunk::Uniform(light_value) = new_chunk {
                if light_value.is_sunlight() {
                    // Send light updates to adjacent chunks
                    let below_position = ChunkFace::Bottom.offset_position(chunk.position);
                    if light_map.chunks.get(&below_position).is_some() {
                        let below_light_updates = light_updates
                            .chunks
                            .entry(below_position)
                            .or_insert(VecDeque::with_capacity(CHUNK_SIZE.pow(3)));
                        for x in 0..CHUNK_SIZE {
                            for z in 0..CHUNK_SIZE {
                                let index = x << 8 | z << 4;
                                below_light_updates.push_front(LightPropagationUpdate {
                                    index: index | 15,
                                    propagation: PropagationType::Add(
                                        new_chunk[index].decrement_artificial(),
                                    ),
                                });
                            }
                        }
                    }

                    let right_position = ChunkFace::Right.offset_position(chunk.position);
                    if light_map.chunks.get(&right_position).is_some() {
                        let right_light_updates = light_updates
                            .chunks
                            .entry(right_position)
                            .or_insert(VecDeque::with_capacity(CHUNK_SIZE.pow(3)));
                        for z in 0..CHUNK_SIZE {
                            for y in 0..CHUNK_SIZE {
                                let index = z << 4 | y;
                                right_light_updates.push_front(LightPropagationUpdate {
                                    index,
                                    propagation: PropagationType::Add(
                                        new_chunk[index | 15 << 8].decrement(),
                                    ),
                                });
                            }
                        }
                    }

                    let left_position = ChunkFace::Left.offset_position(chunk.position);
                    if light_map.chunks.get(&left_position).is_some() {
                        let left_light_updates = light_updates
                            .chunks
                            .entry(left_position)
                            .or_insert(VecDeque::with_capacity(CHUNK_SIZE.pow(3)));
                        for z in 0..CHUNK_SIZE {
                            for y in 0..CHUNK_SIZE {
                                let index = z << 4 | y;
                                left_light_updates.push_front(LightPropagationUpdate {
                                    index: index | 15 << 8,
                                    propagation: PropagationType::Add(new_chunk[index].decrement()),
                                });
                            }
                        }
                    }

                    let front_position = ChunkFace::Front.offset_position(chunk.position);
                    if light_map.chunks.get(&front_position).is_some() {
                        let front_light_updates = light_updates
                            .chunks
                            .entry(front_position)
                            .or_insert(VecDeque::with_capacity(CHUNK_SIZE.pow(3)));
                        for x in 0..CHUNK_SIZE {
                            for y in 0..CHUNK_SIZE {
                                let index = x << 8 | y;
                                front_light_updates.push_front(LightPropagationUpdate {
                                    index,
                                    propagation: PropagationType::Add(
                                        new_chunk[index | 15 << 4].decrement(),
                                    ),
                                });
                            }
                        }
                    }

                    let back_position = ChunkFace::Back.offset_position(chunk.position);
                    if light_map.chunks.get(&back_position).is_some() {
                        let back_light_updates = light_updates
                            .chunks
                            .entry(back_position)
                            .or_insert(VecDeque::with_capacity(CHUNK_SIZE.pow(3)));
                        for x in 0..CHUNK_SIZE {
                            for y in 0..CHUNK_SIZE {
                                let index = x << 8 | y;
                                back_light_updates.push_front(LightPropagationUpdate {
                                    index: index | 15 << 4,
                                    propagation: PropagationType::Add(new_chunk[index].decrement()),
                                });
                            }
                        }
                    }
                } else {
                    // TODO: I'm unsure if this will even happen. If it does it needs to be expanded to send
                    // light updates into adjacent chunks.
                    // Since all empty chunks that are inserted when nothing is above them is treated as
                    // sunlight. When this assumption is wrong, we need to propagate downwards that they
                    // are now not sunlight.
                    //let mut below_position = ChunkFace::Bottom.offset_position(chunk.position);
                    //while let Some(below_light_chunk) = light_map.chunks.get_mut(&below_position) {
                    //    match below_light_chunk {
                    //        LightChunk::Uniform(light) => {
                    //            *light = Light::new(0, 0);
                    //            below_position = below_position - IVec3::Y;
                    //        }
                    //        LightChunk::Normal(_) => {
                    //            let chunk_light_updates = light_updates
                    //                .chunks
                    //                .entry(below_position)
                    //                .or_insert(VecDeque::with_capacity(CHUNK_SIZE.pow(3)));
                    //            for x in 0..CHUNK_SIZE {
                    //                for z in 0..CHUNK_SIZE {
                    //                    chunk_light_updates.push_front(LightPropagationUpdate {
                    //                        index: x << 8 | z << 4 | 15,
                    //                        propagation: PropagationType::Remove(Light::new(15, 0)),
                    //                    });
                    //                }
                    //            }
                    //            break;
                    //        }
                    //    }
                    //}
                }
            }

            let chunk_light_updates = light_updates
                .chunks
                .entry(chunk.position)
                .or_insert(VecDeque::with_capacity(CHUNK_SIZE.pow(3)));

            if let Some(LightChunk::Normal(below_light_chunk)) = light_map
                .chunks
                .get(&ChunkFace::Bottom.offset_position(chunk.position))
            {
                for x in 0..CHUNK_SIZE {
                    for z in 0..CHUNK_SIZE {
                        let index = x << 8 | z << 4;
                        if below_light_chunk[index | 15].can_propagate() {
                            chunk_light_updates.push_front(LightPropagationUpdate {
                                index,
                                propagation: PropagationType::Add(
                                    below_light_chunk[index | 15].decrement(),
                                ),
                            });
                        }
                    }
                }
            }

            if let Some(LightChunk::Normal(right_light_chunk)) = light_map
                .chunks
                .get(&ChunkFace::Right.offset_position(chunk.position))
            {
                for z in 0..CHUNK_SIZE {
                    for y in 0..CHUNK_SIZE {
                        let index = z << 4 | y;
                        if right_light_chunk[index].can_propagate() {
                            chunk_light_updates.push_front(LightPropagationUpdate {
                                index: index | 15 << 8,
                                propagation: PropagationType::Add(
                                    right_light_chunk[index].decrement(),
                                ),
                            });
                        }
                    }
                }
            }

            if let Some(LightChunk::Normal(left_light_chunk)) = light_map
                .chunks
                .get(&ChunkFace::Left.offset_position(chunk.position))
            {
                for z in 0..CHUNK_SIZE {
                    for y in 0..CHUNK_SIZE {
                        let index = z << 4 | y;
                        if left_light_chunk[index | 15 << 8].can_propagate() {
                            chunk_light_updates.push_front(LightPropagationUpdate {
                                index,
                                propagation: PropagationType::Add(
                                    left_light_chunk[index | 15 << 8].decrement(),
                                ),
                            });
                        }
                    }
                }
            }

            if let Some(LightChunk::Normal(front_light_chunk)) = light_map
                .chunks
                .get(&ChunkFace::Front.offset_position(chunk.position))
            {
                for x in 0..CHUNK_SIZE {
                    for y in 0..CHUNK_SIZE {
                        let index = x << 8 | y;
                        if front_light_chunk[index].can_propagate() {
                            chunk_light_updates.push_front(LightPropagationUpdate {
                                index: index | 15 << 4,
                                propagation: PropagationType::Add(front_light_chunk[index]),
                            });
                        }
                    }
                }
            }

            if let Some(LightChunk::Normal(back_light_chunk)) = light_map
                .chunks
                .get(&ChunkFace::Back.offset_position(chunk.position))
            {
                for x in 0..CHUNK_SIZE {
                    for y in 0..CHUNK_SIZE {
                        let index = x << 8 | y;
                        if back_light_chunk[index | 15 << 4].can_propagate() {
                            chunk_light_updates.push_front(LightPropagationUpdate {
                                index,
                                propagation: PropagationType::Add(
                                    back_light_chunk[index | 15 << 4],
                                ),
                            });
                        }
                    }
                }
            }

            light_map.chunks.insert(chunk.position, new_chunk);
        }
    }
}

fn propagate_light(
    world_map: Res<WorldMap>,
    mut light_updates: ResMut<LightUpdateQueue>,
    mut light_map: ResMut<LightMap>,
    mut chunk_mesh_events: EventWriter<TestFinishedLightingEvent>,
) {
    let blocks = Blocks::get();

    // Limit number of updates applied per system loop
    let chunk_positions: Vec<_> = light_updates.chunks.keys().cloned().collect();
    for chunk_position in chunk_positions.iter() {
        let mut updates = light_updates.chunks.remove(chunk_position).unwrap();

        // Because of the unordered execution we discard updates that don't have an associated
        // LightChunk (as they are guaranteed not part of a chunk), but keep updates where the
        // chunk has not been added yet (as that may happen after).
        let Some(light_chunk) = light_map.chunks.get_mut(chunk_position) else { continue; };
        let Some(chunk) = world_map.get_chunk(chunk_position) else { light_updates.chunks.insert(*chunk_position, updates); continue };

        if chunk.is_uniform() && !blocks[&chunk[0]].is_transparent() {
            // Ignore light updates sent into solid chunks.
            updates.clear();
        }

        while let Some(update) = updates.pop_back() {
            let mut last = None;
            let light = match light_chunk {
                LightChunk::Uniform(uniform_light) => {
                    if let PropagationType::Add(update_light) = update.propagation {
                        if update_light.sunlight() > uniform_light.sunlight()
                            || update_light.artificial() > 0
                        {
                            *light_chunk =
                                LightChunk::Normal(vec![*uniform_light; CHUNK_SIZE.pow(3)]);
                            match light_chunk {
                                LightChunk::Normal(inner) => &mut inner[update.index],
                                _ => unreachable!(),
                            }
                        } else {
                            continue;
                        }
                    } else {
                        last = Some(*uniform_light);
                        *light_chunk = LightChunk::Normal(vec![*uniform_light; CHUNK_SIZE.pow(3)]);
                        match light_chunk {
                            LightChunk::Normal(inner) => &mut inner[update.index],
                            _ => unreachable!(),
                        }
                    }
                }
                LightChunk::Normal(light_chunk) => &mut light_chunk[update.index],
            };

            let block_config = &blocks[&chunk[update.index]];

            let propagation = match update.propagation {
                PropagationType::Add(update_light) => {
                    if block_config.is_transparent() {
                        let mut changed = false;
                        if update_light.sunlight() > light.sunlight() {
                            light.set_sunlight(update_light.sunlight());
                            changed = true;
                        }

                        if update_light.artificial() > light.artificial() {
                            light.set_artificial(update_light.artificial());
                            changed = true;
                        }

                        if changed && light.can_propagate() {
                            Some((
                                PropagationType::Add(light.decrement()),
                                PropagationType::Add(light.decrement_artificial().decrement_sun(
                                    if light.sunlight() == 15 {
                                        block_config.light_attenuation()
                                    } else {
                                        // Sunlight levels below 15 spread like artificial light
                                        block_config.light_attenuation().max(1)
                                    },
                                )),
                            ))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
                PropagationType::Remove(update_light) => {
                    if update_light.sunlight() == 0 {
                        // This is a special case that will(should) happen when a block is placed,
                        // signaling that this block needs to be floodfilled.
                        let propagation = if light.can_propagate() {
                            Some((
                                PropagationType::Remove(*light),
                                // Sunlight needs to be re-propagated all the way down.
                                // TODO: This needs to take into account artificial light.
                                PropagationType::Remove(Light::new(0, 0)),
                            ))
                        } else {
                            Some((
                                PropagationType::Remove(Light::new(1, 1)),
                                PropagationType::Remove(Light::new(1, 1)),
                            ))
                        };

                        *light = Light::new(0, 0);

                        propagation
                    } else if update_light.sunlight() == 1 && update_light.artificial() == 1 {
                        // This is for the above only.
                        if light.can_propagate() {
                            Some((
                                PropagationType::Add(light.decrement()),
                                PropagationType::Add(light.decrement_artificial().decrement_sun(
                                    if light.sunlight() == 15 {
                                        block_config.light_attenuation()
                                    } else {
                                        // Sunlight levels below 15 spread like artificial light
                                        block_config.light_attenuation().max(1)
                                    },
                                )),
                            ))
                        } else {
                            None
                        }
                    } else if update_light.sunlight() > light.sunlight()
                        || update_light.artificial() > light.artificial()
                    {
                        let propagation = if light.can_propagate() {
                            Some((
                                PropagationType::Remove(*light),
                                PropagationType::Remove(*light),
                            ))
                        } else {
                            None
                        };

                        *light = Light::new(0, 0);

                        propagation
                    } else if light.can_propagate() {
                        Some((
                            PropagationType::Add(light.decrement()),
                            PropagationType::Add(light.decrement_artificial().decrement_sun(
                                if light.sunlight() == 15 {
                                    block_config.light_attenuation()
                                } else {
                                    // Sunlight levels below 15 spread like artificial light
                                    block_config.light_attenuation().max(1)
                                },
                            )),
                        ))
                    } else {
                        None
                    }
                }
            };

            if let Some((propagation, down_propagation)) = propagation {
                let position = IVec3::new(
                    ((update.index & 0b1111_0000_0000) >> 8) as i32,
                    (update.index & 0b0000_0000_1111) as i32,
                    ((update.index & 0b0000_1111_0000) >> 4) as i32,
                );

                let (chunk_up, index) =
                    utils::world_position_to_chunk_position_and_block_index(position + IVec3::Y);
                if chunk_up.y == 16 {
                    let top_updates = light_updates
                        .chunks
                        .entry(ChunkFace::Top.offset_position(*chunk_position))
                        .or_insert(VecDeque::with_capacity(CHUNK_SIZE.pow(3)));
                    top_updates.push_front(LightPropagationUpdate { index, propagation });
                } else {
                    updates.push_front(LightPropagationUpdate { index, propagation });
                }

                let (chunk_down, index) =
                    utils::world_position_to_chunk_position_and_block_index(position - IVec3::Y);
                if chunk_down.y == -16 {
                    let bottom_updates = light_updates
                        .chunks
                        .entry(ChunkFace::Bottom.offset_position(*chunk_position))
                        .or_insert(VecDeque::with_capacity(CHUNK_SIZE.pow(3)));

                    bottom_updates.push_back(LightPropagationUpdate {
                        index,
                        propagation: down_propagation,
                    });
                } else {
                    updates.push_back(LightPropagationUpdate {
                        index,
                        propagation: down_propagation,
                    });
                }

                let (chunk_right, index) =
                    utils::world_position_to_chunk_position_and_block_index(position + IVec3::X);
                if chunk_right.x == 16 {
                    let right_updates = light_updates
                        .chunks
                        .entry(ChunkFace::Right.offset_position(*chunk_position))
                        .or_insert(VecDeque::with_capacity(CHUNK_SIZE.pow(3)));
                    right_updates.push_front(LightPropagationUpdate { index, propagation });
                } else {
                    updates.push_front(LightPropagationUpdate { index, propagation });
                }

                let (chunk_left, index) =
                    utils::world_position_to_chunk_position_and_block_index(position - IVec3::X);
                if chunk_left.x == -16 {
                    let left_updates = light_updates
                        .chunks
                        .entry(ChunkFace::Left.offset_position(*chunk_position))
                        .or_insert(VecDeque::with_capacity(CHUNK_SIZE.pow(3)));
                    left_updates.push_front(LightPropagationUpdate { index, propagation });
                } else {
                    updates.push_front(LightPropagationUpdate { index, propagation });
                }

                let (chunk_front, index) =
                    utils::world_position_to_chunk_position_and_block_index(position + IVec3::Z);
                if chunk_front.z == 16 {
                    let front_updates = light_updates
                        .chunks
                        .entry(ChunkFace::Front.offset_position(*chunk_position))
                        .or_insert(VecDeque::with_capacity(CHUNK_SIZE.pow(3)));
                    front_updates.push_front(LightPropagationUpdate { index, propagation });
                } else {
                    updates.push_front(LightPropagationUpdate { index, propagation });
                }

                let (chunk_back, index) =
                    utils::world_position_to_chunk_position_and_block_index(position - IVec3::Z);
                if chunk_back.z == -16 {
                    let back_updates = light_updates
                        .chunks
                        .entry(ChunkFace::Back.offset_position(*chunk_position))
                        .or_insert(VecDeque::with_capacity(CHUNK_SIZE.pow(3)));
                    back_updates.push_front(LightPropagationUpdate { index, propagation });
                } else {
                    updates.push_front(LightPropagationUpdate { index, propagation });
                }
            }
        }

        chunk_mesh_events.send(TestFinishedLightingEvent(*chunk_position));
        chunk_mesh_events.send(TestFinishedLightingEvent(
            *chunk_position + IVec3::new(CHUNK_SIZE as i32, 0, 0),
        ));
        chunk_mesh_events.send(TestFinishedLightingEvent(
            *chunk_position - IVec3::new(CHUNK_SIZE as i32, 0, 0),
        ));
        chunk_mesh_events.send(TestFinishedLightingEvent(
            *chunk_position + IVec3::new(0, CHUNK_SIZE as i32, 0),
        ));
        chunk_mesh_events.send(TestFinishedLightingEvent(
            *chunk_position - IVec3::new(0, CHUNK_SIZE as i32, 0),
        ));
        chunk_mesh_events.send(TestFinishedLightingEvent(
            *chunk_position + IVec3::new(0, 0, CHUNK_SIZE as i32),
        ));
        chunk_mesh_events.send(TestFinishedLightingEvent(
            *chunk_position - IVec3::new(0, 0, CHUNK_SIZE as i32),
        ));
    }
}

fn light_chunk_unloading(world_map: Res<WorldMap>, mut light_map: ResMut<LightMap>) {
    for position in light_map.chunks.keys().cloned().collect::<Vec<_>>().iter() {
        if !world_map.contains_chunk(position) {
            light_map.chunks.remove(position);
        }
    }
}

struct TestFinishedLightingEvent(IVec3);

fn send_chunk_mesh_events(
    lighting_updates: Res<LightUpdateQueue>,
    mut lighting_events: EventReader<TestFinishedLightingEvent>,
    mut chunk_mesh_events: EventWriter<ChunkMeshEvent>,
) {
    for light_event in lighting_events.iter() {
        let position = light_event.0;
        if lighting_updates.chunks.get(&position).is_none()
            && !lighting_updates
                .chunks
                .contains_key(&(position + IVec3::new(0, CHUNK_SIZE as i32, 0)))
            && !lighting_updates
                .chunks
                .contains_key(&(position - IVec3::new(0, CHUNK_SIZE as i32, 0)))
            && !lighting_updates
                .chunks
                .contains_key(&(position + IVec3::new(CHUNK_SIZE as i32, 0, 0)))
            && !lighting_updates
                .chunks
                .contains_key(&(position - IVec3::new(CHUNK_SIZE as i32, 0, 0)))
            && !lighting_updates
                .chunks
                .contains_key(&(position + IVec3::new(0, 0, CHUNK_SIZE as i32)))
            && !lighting_updates
                .chunks
                .contains_key(&(position - IVec3::new(0, 0, CHUNK_SIZE as i32)))
        {
            chunk_mesh_events.send(ChunkMeshEvent { position });
        }
    }
}
