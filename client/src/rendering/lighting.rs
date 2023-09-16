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
        world_map::{chunk::ChunkFace, WorldMap},
        Origin,
    },
};

use super::chunk::{ChunkMeshEvent, ExpandedLightChunk};

pub struct LightingPlugin;
impl Plugin for LightingPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(LightMap::default())
            .add_event::<TestFinishedLightingEvent>()
            .add_event::<RelightEvent>()
            .add_event::<FailedLightingEvent>()
            .insert_resource(LightUpdateQueues::default())
            .add_systems(
                Update,
                (
                    // TODO: I intitially thought events were synced on frame update, but it seems
                    // like it does it instantly? Here, queue_chunk_updates iterates over the
                    // ChunkResponse event, and then inserts a RelightEvent that I expect to be
                    // read the next cycle, because the chunk needs to be inserted in the world
                    // map on the same frame so it is available. Instead, it can the happen that it
                    // will run relight_chunks before this (the same update), and then
                    // read the event that was just inserted. I have probably used this assumption
                    // elsewhere, so there is bound to be unexpected race conditions.
                    //
                    // Need to be run after to make sure chunks/blocks have a frame to be added to
                    // the world map.
                    queue_chunk_updates.after(relight_chunks),
                    queue_block_updates.after(relight_chunks),
                    handle_failed,
                    propagate_light,
                    relight_chunks,
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
        let (chunk_pos, block_index) =
            utils::world_position_to_chunk_position_and_block_index(block_position);
        if let Some(light_chunk) = self.chunks.get(&chunk_pos) {
            Some(light_chunk[block_index])
        } else {
            None
        }
    }
    pub fn get_expanded_chunk(&self, position: IVec3) -> ExpandedLightChunk {
        let center = self.chunks.get(&position).unwrap().clone();

        let top_position = position + IVec3::new(0, CHUNK_SIZE as i32, 0);
        let top_chunk = self.chunks.get(&top_position);

        let bottom_position = position - IVec3::new(0, CHUNK_SIZE as i32, 0);
        let bottom_chunk = self.chunks.get(&bottom_position);

        let right_position = position + IVec3::new(CHUNK_SIZE as i32, 0, 0);
        let right_chunk = self.chunks.get(&right_position);

        let left_position = position - IVec3::new(CHUNK_SIZE as i32, 0, 0);
        let left_chunk = self.chunks.get(&left_position);

        let front_position = position + IVec3::new(0, 0, CHUNK_SIZE as i32);
        let front_chunk = self.chunks.get(&front_position);

        let back_position = position - IVec3::new(0, 0, CHUNK_SIZE as i32);
        let back_chunk = self.chunks.get(&back_position);

        // XXX: The lights default to zero to avoid having to wrap them in Option, if the
        // corresponding block is None, the light will be irrelevant.
        let mut top: [[Light; CHUNK_SIZE]; CHUNK_SIZE] = Default::default();
        let mut bottom: [[Light; CHUNK_SIZE]; CHUNK_SIZE] = Default::default();
        let mut right: [[Light; CHUNK_SIZE]; CHUNK_SIZE] = Default::default();
        let mut left: [[Light; CHUNK_SIZE]; CHUNK_SIZE] = Default::default();
        let mut front: [[Light; CHUNK_SIZE]; CHUNK_SIZE] = Default::default();
        let mut back: [[Light; CHUNK_SIZE]; CHUNK_SIZE] = Default::default();

        for i in 0..CHUNK_SIZE {
            for j in 0..CHUNK_SIZE {
                if let Some(top_chunk) = top_chunk {
                    top[i][j] = top_chunk[[i, 0, j]];
                }
                if let Some(bottom_chunk) = bottom_chunk {
                    bottom[i][j] = bottom_chunk[[i, CHUNK_SIZE - 1, j]];
                }
                if let Some(right_chunk) = right_chunk {
                    right[i][j] = right_chunk[[0, i, j]];
                }
                if let Some(left_chunk) = left_chunk {
                    left[i][j] = left_chunk[[CHUNK_SIZE - 1, i, j]];
                }
                if let Some(back_chunk) = back_chunk {
                    back[i][j] = back_chunk[[i, j, CHUNK_SIZE - 1]];
                }
                if let Some(front_chunk) = front_chunk {
                    front[i][j] = front_chunk[[i, j, 0]];
                }
            }
        }

        return ExpandedLightChunk {
            center,
            top,
            bottom,
            right,
            left,
            front,
            back,
        };
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

// Event sent whenever a chunk is added, or a block changes in a chunk.
#[derive(Event)]
struct RelightEvent {
    chunk_position: IVec3,
}

// TODO: This is extremely expensive when rendering large underground caverns because it will
// trigger on almost every chunk column. I think it's as simple a fix as assuming that all chunks
// below a certain y level is never sunlight.
//
// Lighting is inherently faulty because it needs to assume what is sky and what is not. When this
// assumption is wrong, this event is sent to relight the chunk and all its surrounding chunks.
#[derive(Event)]
struct FailedLightingEvent {
    chunk_position: IVec3,
}

struct LightPropagation {
    // Index in the chunk the update is to be applied to.
    index: usize,
    // Light value of the block this update originated from.
    light: Light,
    // Marker needed for sunlight, so as to not decrement it when propagating downwards.
    vertical: bool,
}

// To cut down on lighting calculations from sunlight it uses a ring buffer. When light
// propagates down it is placed at the back of the queue, and when in any other direction at
// the front. When it then does pop_back, it will always pick the sunlight with the highest
// priority.
#[derive(Resource, Default, DerefMut, Deref)]
struct LightUpdateQueues(HashMap<IVec3, VecDeque<LightPropagation>>);

fn queue_block_updates(
    light_map: Res<LightMap>,
    mut block_updates: EventReader<NetworkData<messages::BlockUpdates>>,
    // Reuse FailedLightingEvent since it's the exact same operation, I can't think of a good name
    // for it.
    mut failed_lighting_events: EventWriter<FailedLightingEvent>,
) {
    for block_update in block_updates.read() {
        let mut chunk_position = block_update.chunk_position;
        while let Some(light_chunk) = light_map.chunks.get(&chunk_position) {
            if matches!(light_chunk, LightChunk::Uniform(light) if light.sunlight() == 0) {
                break;
            }

            failed_lighting_events.send(FailedLightingEvent { chunk_position });
            chunk_position = ChunkFace::Bottom.shift_position(chunk_position);
        }
    }
}

fn queue_chunk_updates(
    mut chunk_responses: EventReader<NetworkData<messages::ChunkResponse>>,
    mut relight_events: EventWriter<RelightEvent>,
) {
    for response in chunk_responses.read() {
        for chunk in response.chunks.iter() {
            relight_events.send(RelightEvent {
                chunk_position: chunk.position,
            });
        }
    }
}

fn handle_failed(
    light_map: Res<LightMap>,
    mut failed_lighting_events: EventReader<FailedLightingEvent>,
    mut relight_events: EventWriter<RelightEvent>,
) {
    for failed in failed_lighting_events.read() {
        for x in [1, 0, -1] {
            for y in [1, 0, -1] {
                for z in [1, 0, -1] {
                    let chunk_position = failed.chunk_position + IVec3 { x, y, z };
                    if light_map.chunks.contains_key(&chunk_position) {
                        relight_events.send(RelightEvent { chunk_position });
                    }
                }
            }
        }
    }
}

fn relight_chunks(
    mut light_map: ResMut<LightMap>,
    world_map: Res<WorldMap>,
    mut light_update_queues: ResMut<LightUpdateQueues>,
    mut relight_events: EventReader<RelightEvent>,
    mut failed_lighting_events: EventWriter<FailedLightingEvent>,
) {
    let blocks = Blocks::get();

    for relight_event in relight_events.read() {
        let Some(chunk) = world_map.get_chunk(&relight_event.chunk_position) else {
            continue;
        };

        let mut new_chunk = if chunk.is_uniform() {
            let block_config = &blocks[&chunk[0]];
            if block_config.light_attenuation() == 0 {
                LightChunk::Uniform(Light::new(15, 0))
            } else {
                LightChunk::Uniform(Light::new(0, 0))
            }
        } else {
            LightChunk::Normal(vec![Light::new(0, 0); CHUNK_SIZE.pow(3)])
        };

        if let Some(above_light_chunk) = light_map
            .chunks
            .get(&ChunkFace::Top.shift_position(relight_event.chunk_position))
        {
            if !matches!(above_light_chunk, LightChunk::Uniform(light) if light.sunlight() == 15)
                && matches!(new_chunk, LightChunk::Uniform(_))
            {
                // If the above chunk is uniform light level 0 or a normal light chunk, this new
                // chunk below should be light level 0 too.
                new_chunk = LightChunk::Uniform(Light::new(0, 0));
            }

            if !matches!(above_light_chunk, LightChunk::Uniform(light) if light.sunlight() == 0) {
                let chunk_light_update_queue = light_update_queues
                    .entry(relight_event.chunk_position)
                    .or_insert(VecDeque::with_capacity(CHUNK_SIZE.pow(3)));
                for x in 0..CHUNK_SIZE {
                    for z in 0..CHUNK_SIZE {
                        chunk_light_update_queue.push_back(LightPropagation {
                            index: x << 8 | z << 4 | 15,
                            light: match above_light_chunk {
                                LightChunk::Uniform(light) => *light,
                                LightChunk::Normal(light) => light[x << 8 | z << 4],
                            },
                            vertical: true,
                        });
                    }
                }
            }
        }

        if matches!(new_chunk, LightChunk::Uniform(light) if light.sunlight() == 15) {
            // If the new light chunk is uniform sunlight, there won't exist any propagation
            // updates, so sending to adjacent chunks has to be done manually.
            let below_position = ChunkFace::Bottom.shift_position(relight_event.chunk_position);
            if light_map.chunks.get(&below_position).is_some() {
                let below_light_updates = light_update_queues
                    .entry(below_position)
                    .or_insert(VecDeque::with_capacity(CHUNK_SIZE.pow(3)));
                for x in 0..CHUNK_SIZE {
                    for z in 0..CHUNK_SIZE {
                        let index = x << 8 | z << 4;

                        if !new_chunk[index].can_propagate() {
                            continue;
                        }

                        below_light_updates.push_back(LightPropagation {
                            index: index | 15,
                            light: new_chunk[index],
                            vertical: true,
                        });
                    }
                }
            }

            let right_position = ChunkFace::Right.shift_position(relight_event.chunk_position);
            if light_map.chunks.get(&right_position).is_some() {
                let right_light_updates = light_update_queues
                    .entry(right_position)
                    .or_insert(VecDeque::with_capacity(CHUNK_SIZE.pow(3)));
                for z in 0..CHUNK_SIZE {
                    for y in 0..CHUNK_SIZE {
                        let index = z << 4 | y;

                        if !new_chunk[index].can_propagate() {
                            continue;
                        }

                        right_light_updates.push_front(LightPropagation {
                            index,
                            light: new_chunk[index | 15 << 8],
                            vertical: false,
                        });
                    }
                }
            }

            let left_position = ChunkFace::Left.shift_position(relight_event.chunk_position);
            if light_map.chunks.get(&left_position).is_some() {
                let left_light_updates = light_update_queues
                    .entry(left_position)
                    .or_insert(VecDeque::with_capacity(CHUNK_SIZE.pow(3)));
                for z in 0..CHUNK_SIZE {
                    for y in 0..CHUNK_SIZE {
                        let index = z << 4 | y;

                        if !new_chunk[index].can_propagate() {
                            continue;
                        }

                        left_light_updates.push_front(LightPropagation {
                            index: index | 15 << 8,
                            light: new_chunk[index],
                            vertical: false,
                        });
                    }
                }
            }

            let front_position = ChunkFace::Front.shift_position(relight_event.chunk_position);
            if light_map.chunks.get(&front_position).is_some() {
                let front_light_updates = light_update_queues
                    .entry(front_position)
                    .or_insert(VecDeque::with_capacity(CHUNK_SIZE.pow(3)));
                for x in 0..CHUNK_SIZE {
                    for y in 0..CHUNK_SIZE {
                        let index = x << 8 | y;

                        if !new_chunk[index].can_propagate() {
                            continue;
                        }

                        front_light_updates.push_front(LightPropagation {
                            index,
                            light: new_chunk[index | 15 << 4],
                            vertical: false,
                        });
                    }
                }
            }

            let back_position = ChunkFace::Back.shift_position(relight_event.chunk_position);
            if light_map.chunks.get(&back_position).is_some() {
                let back_light_updates = light_update_queues
                    .entry(back_position)
                    .or_insert(VecDeque::with_capacity(CHUNK_SIZE.pow(3)));
                for x in 0..CHUNK_SIZE {
                    for y in 0..CHUNK_SIZE {
                        let index = x << 8 | y;

                        if !new_chunk[index].can_propagate() {
                            continue;
                        }

                        back_light_updates.push_front(LightPropagation {
                            index: index | 15 << 4,
                            light: new_chunk[index],
                            vertical: false,
                        });
                    }
                }
            }
        } else {
            // Since all empty chunks that are inserted when nothing is above them is treated as
            // sunlight. When this assumption is wrong, we need to propagate downwards that they
            // are now not sunlight.
            let mut failed = false;
            let mut below_position = ChunkFace::Bottom.shift_position(relight_event.chunk_position);
            while let Some(below_light_chunk) = light_map.chunks.get(&below_position) {
                if !failed && matches!(below_light_chunk, LightChunk::Normal(_)) {
                    break;
                } else if matches!(below_light_chunk, LightChunk::Uniform(light) if light.sunlight() == 0)
                {
                    break;
                } else {
                    failed = true;
                }

                failed_lighting_events.send(FailedLightingEvent {
                    chunk_position: below_position,
                });
                below_position = ChunkFace::Bottom.shift_position(below_position);
            }
        }

        let chunk_light_updates = light_update_queues
            .entry(relight_event.chunk_position)
            .or_insert(VecDeque::with_capacity(CHUNK_SIZE.pow(3)));

        if let Some(LightChunk::Normal(below_light_chunk)) = light_map
            .chunks
            .get(&ChunkFace::Bottom.shift_position(relight_event.chunk_position))
        {
            for x in 0..CHUNK_SIZE {
                for z in 0..CHUNK_SIZE {
                    let index = x << 8 | z << 4;
                    if below_light_chunk[index | 15].can_propagate() {
                        chunk_light_updates.push_front(LightPropagation {
                            index,
                            light: below_light_chunk[index | 15],
                            vertical: false,
                        });
                    }
                }
            }
        }

        if let Some(LightChunk::Normal(right_light_chunk)) = light_map
            .chunks
            .get(&ChunkFace::Right.shift_position(relight_event.chunk_position))
        {
            for z in 0..CHUNK_SIZE {
                for y in 0..CHUNK_SIZE {
                    let index = z << 4 | y;
                    if right_light_chunk[index].can_propagate() {
                        chunk_light_updates.push_front(LightPropagation {
                            index: index | 15 << 8,
                            light: right_light_chunk[index],
                            vertical: false,
                        });
                    }
                }
            }
        }

        if let Some(LightChunk::Normal(left_light_chunk)) = light_map
            .chunks
            .get(&ChunkFace::Left.shift_position(relight_event.chunk_position))
        {
            for z in 0..CHUNK_SIZE {
                for y in 0..CHUNK_SIZE {
                    let index = z << 4 | y;
                    if left_light_chunk[index | 15 << 8].can_propagate() {
                        chunk_light_updates.push_front(LightPropagation {
                            index,
                            light: left_light_chunk[index | 15 << 8],
                            vertical: false,
                        });
                    }
                }
            }
        }

        if let Some(LightChunk::Normal(front_light_chunk)) = light_map
            .chunks
            .get(&ChunkFace::Front.shift_position(relight_event.chunk_position))
        {
            for x in 0..CHUNK_SIZE {
                for y in 0..CHUNK_SIZE {
                    let index = x << 8 | y;
                    if front_light_chunk[index].can_propagate() {
                        chunk_light_updates.push_front(LightPropagation {
                            index: index | 15 << 4,
                            light: front_light_chunk[index],
                            vertical: false,
                        });
                    }
                }
            }
        }

        if let Some(LightChunk::Normal(back_light_chunk)) = light_map
            .chunks
            .get(&ChunkFace::Back.shift_position(relight_event.chunk_position))
        {
            for x in 0..CHUNK_SIZE {
                for y in 0..CHUNK_SIZE {
                    let index = x << 8 | y;
                    if back_light_chunk[index | 15 << 4].can_propagate() {
                        chunk_light_updates.push_front(LightPropagation {
                            index,
                            light: back_light_chunk[index | 15 << 4],
                            vertical: false,
                        });
                    }
                }
            }
        }

        light_map
            .chunks
            .insert(relight_event.chunk_position, new_chunk);
    }
}

fn propagate_light(
    world_map: Res<WorldMap>,
    mut light_update_queues: ResMut<LightUpdateQueues>,
    mut light_map: ResMut<LightMap>,
    mut chunk_mesh_events: EventWriter<TestFinishedLightingEvent>,
) {
    let blocks = Blocks::get();

    // Limit number of updates applied per system loop
    let chunk_positions: Vec<_> = light_update_queues.keys().cloned().collect();
    for chunk_position in chunk_positions.iter() {
        // Sunlight from the sides is often inserted as light updates before any coming from
        // above. This causes some nasty cascading, so needs to be circumvented. By not processing
        // the updates before the chunk above is present, and its update have finished, we can be
        // certain that the sunlight from above has been inserted into the update queue. Ignore
        // uniform chunks
        //let above_pos = ChunkFace::Top.offset_position(*chunk_position);
        //if !matches!(
        //    light_map.chunks.get(&chunk_position),
        //    Some(LightChunk::Uniform(_))
        //) && (light_updates.chunks.contains_key(&above_pos)
        //    || !light_map.chunks.contains_key(&above_pos))
        //{
        //    continue;
        //}

        let mut updates = light_update_queues.remove(chunk_position).unwrap();

        // Because of the unordered execution we discard updates that don't have an associated
        // LightChunk (as they are guaranteed not part of a chunk), but keep updates where the
        // chunk has not been added yet (as that may happen after).
        let Some(light_chunk) = light_map.chunks.get_mut(chunk_position) else {
            continue;
        };
        let Some(chunk) = world_map.get_chunk(chunk_position) else {
            light_update_queues.insert(*chunk_position, updates);
            continue;
        };

        if chunk.is_uniform() && blocks[&chunk[0]].light_attenuation() == 15 {
            // Ignore light updates sent into solid chunks, but trigger render.
            updates.clear();
        }

        while let Some(update) = updates.pop_back() {
            let light = match light_chunk {
                LightChunk::Uniform(uniform_light) => {
                    if update.light.sunlight() > uniform_light.sunlight()
                        || update.light.artificial() > 0
                    {
                        *light_chunk = LightChunk::Normal(vec![*uniform_light; CHUNK_SIZE.pow(3)]);
                        match light_chunk {
                            LightChunk::Normal(inner) => &mut inner[update.index],
                            _ => unreachable!(),
                        }
                    } else {
                        continue;
                    }
                }
                LightChunk::Normal(light_chunk) => &mut light_chunk[update.index],
            };

            let block_config = &blocks[&chunk[update.index]];

            if block_config.light_attenuation() == 15 {
                continue;
            }

            let sun_decrement = if update.vertical
                && update.light.sunlight() == 15
                && block_config.light_attenuation() == 0
            {
                0
            } else {
                block_config.light_attenuation().max(1)
            };
            let new_light = update
                .light
                .decrement_sun(sun_decrement)
                .decrement_artificial();

            let mut changed = false;

            if new_light.sunlight() > light.sunlight() {
                light.set_sunlight(new_light.sunlight());
                changed = true;
            }

            if update.light.artificial() > light.artificial() {
                light.set_artificial(new_light.artificial());
                changed = true;
            }

            if changed {
                let position = IVec3::new(
                    ((update.index & 0b1111_0000_0000) >> 8) as i32,
                    (update.index & 0b0000_0000_1111) as i32,
                    ((update.index & 0b0000_1111_0000) >> 4) as i32,
                );

                let (chunk_up, index) =
                    utils::world_position_to_chunk_position_and_block_index(position + IVec3::Y);
                if chunk_up.y == 16 {
                    let top_updates = light_update_queues
                        .entry(ChunkFace::Top.shift_position(*chunk_position))
                        .or_insert(VecDeque::with_capacity(CHUNK_SIZE.pow(3)));
                    top_updates.push_front(LightPropagation {
                        index,
                        light: *light,
                        vertical: false,
                    })
                } else {
                    updates.push_front(LightPropagation {
                        index,
                        light: *light,
                        vertical: false,
                    })
                }

                let (chunk_down, index) =
                    utils::world_position_to_chunk_position_and_block_index(position - IVec3::Y);
                if chunk_down.y == -16 {
                    let bottom_updates = light_update_queues
                        .entry(ChunkFace::Bottom.shift_position(*chunk_position))
                        .or_insert(VecDeque::with_capacity(CHUNK_SIZE.pow(3)));

                    bottom_updates.push_back(LightPropagation {
                        index,
                        light: *light,
                        vertical: true,
                    })
                } else {
                    updates.push_back(LightPropagation {
                        index,
                        light: *light,
                        vertical: true,
                    })
                }

                let (chunk_right, index) =
                    utils::world_position_to_chunk_position_and_block_index(position + IVec3::X);
                if chunk_right.x == 16 {
                    let right_updates = light_update_queues
                        .entry(ChunkFace::Right.shift_position(*chunk_position))
                        .or_insert(VecDeque::with_capacity(CHUNK_SIZE.pow(3)));
                    right_updates.push_front(LightPropagation {
                        index,
                        light: *light,
                        vertical: false,
                    })
                } else {
                    updates.push_front(LightPropagation {
                        index,
                        light: *light,
                        vertical: false,
                    })
                }

                let (chunk_left, index) =
                    utils::world_position_to_chunk_position_and_block_index(position - IVec3::X);
                if chunk_left.x == -16 {
                    let left_updates = light_update_queues
                        .entry(ChunkFace::Left.shift_position(*chunk_position))
                        .or_insert(VecDeque::with_capacity(CHUNK_SIZE.pow(3)));
                    left_updates.push_front(LightPropagation {
                        index,
                        light: *light,
                        vertical: false,
                    })
                } else {
                    updates.push_front(LightPropagation {
                        index,
                        light: *light,
                        vertical: false,
                    })
                }

                let (chunk_front, index) =
                    utils::world_position_to_chunk_position_and_block_index(position + IVec3::Z);
                if chunk_front.z == 16 {
                    let front_updates = light_update_queues
                        .entry(ChunkFace::Front.shift_position(*chunk_position))
                        .or_insert(VecDeque::with_capacity(CHUNK_SIZE.pow(3)));
                    front_updates.push_front(LightPropagation {
                        index,
                        light: *light,
                        vertical: false,
                    })
                } else {
                    updates.push_front(LightPropagation {
                        index,
                        light: *light,
                        vertical: false,
                    })
                }

                let (chunk_back, index) =
                    utils::world_position_to_chunk_position_and_block_index(position - IVec3::Z);
                if chunk_back.z == -16 {
                    let back_updates = light_update_queues
                        .entry(ChunkFace::Back.shift_position(*chunk_position))
                        .or_insert(VecDeque::with_capacity(CHUNK_SIZE.pow(3)));
                    back_updates.push_front(LightPropagation {
                        index,
                        light: *light,
                        vertical: false,
                    })
                } else {
                    updates.push_front(LightPropagation {
                        index,
                        light: *light,
                        vertical: false,
                    })
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

#[derive(Event)]
struct TestFinishedLightingEvent(IVec3);

fn send_chunk_mesh_events(
    light_update_queues: Res<LightUpdateQueues>,
    mut lighting_events: EventReader<TestFinishedLightingEvent>,
    mut chunk_mesh_events: EventWriter<ChunkMeshEvent>,
) {
    for light_event in lighting_events.read() {
        let position = light_event.0;
        if !light_update_queues.contains_key(&position)
            && !light_update_queues.contains_key(&(position + IVec3::new(0, CHUNK_SIZE as i32, 0)))
            && !light_update_queues.contains_key(&(position - IVec3::new(0, CHUNK_SIZE as i32, 0)))
            && !light_update_queues.contains_key(&(position + IVec3::new(CHUNK_SIZE as i32, 0, 0)))
            && !light_update_queues.contains_key(&(position - IVec3::new(CHUNK_SIZE as i32, 0, 0)))
            && !light_update_queues.contains_key(&(position + IVec3::new(0, 0, CHUNK_SIZE as i32)))
            && !light_update_queues.contains_key(&(position - IVec3::new(0, 0, CHUNK_SIZE as i32)))
        {
            chunk_mesh_events.send(ChunkMeshEvent { position });
        }
    }
}
