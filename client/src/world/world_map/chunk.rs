use std::collections::{HashMap, HashSet};
use std::ops::{Index, IndexMut};

use bevy::{
    prelude::*,
    tasks::{AsyncComputeTaskPool, Task},
};
use fmc_networking::BlockId;
use futures_lite::future;

use crate::world::blocks::Blocks;
use crate::{constants::*, game_state::GameState, utils::Direction, world::world_map::WorldMap};

pub struct ChunkPlugin;
impl Plugin for ChunkPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<VisibleSidesEvent>();
        app.add_systems(
            Update,
            (handle_visible_sides_tasks, visible_sides_system).run_if(in_state(GameState::Playing)),
        );
    }
}

pub struct VisibleSidesEvent(pub IVec3);

#[derive(Component)]
struct VisibleSidesTask(Task<VisibleSides>);

// TODO: Would be really nice to use bevy change detection to trigger this instead of events.
//       Dont' know how, maybe on Mesh change? But only has access to mesh handle.
/// Run whenever a chunk changes.
fn visible_sides_system(
    mut commands: Commands,
    world_map: Res<WorldMap>,
    mut find_visible_sides_events: EventReader<VisibleSidesEvent>,
) {
    let thread_pool = AsyncComputeTaskPool::get();

    for position in find_visible_sides_events.iter() {
        if let Some(chunk) = world_map.get_chunk(&position.0) {
            if let Some(entity) = chunk.entity {
                let task = thread_pool.spawn(VisibleSides::new(chunk.clone()));
                commands.entity(entity).insert(VisibleSidesTask(task));
            }
        } else {
            // This happens a lot maybe just remove
            //warn!(
            //    "Tried to create visible sides async task for a chunk that doesn't exist, at position {}",
            //    &position.0
            //);
        }
    }
}

fn handle_visible_sides_tasks(
    mut commands: Commands,
    mut sides_tasks: Query<(Entity, &mut VisibleSidesTask)>,
) {
    for (entity, mut task) in sides_tasks.iter_mut() {
        if let Some(sides) = future::block_on(future::poll_once(&mut task.0)) {
            commands
                .entity(entity)
                .insert(sides)
                .remove::<VisibleSidesTask>();
        }
    }
}

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
    // Entity in the ECS. Has the mesh and chunk::VisibleSides.
    pub entity: Option<Entity>,
    /// A CHUNK_SIZE^3 array containing all the blocks in the chunk.
    /// Indexed by x*CHUNK_SIZE^2 + y*CHUNK_SIZE + z
    blocks: Vec<BlockId>,
    /// Optional block state containing info like rotation and color
    ///
    /// bits:
    ///     0000 0000 0000 unused
    ///     0000
    ///       ^^-north/east/south/west
    ///      ^---center/side model
    ///     ^----upright / upside down
    block_state: HashMap<usize, u16>,
}

impl Chunk {
    /// Build a normal chunk
    pub fn new(entity: Entity, blocks: Vec<BlockId>, block_state: HashMap<usize, u16>) -> Self {
        return Self {
            entity: Some(entity),
            blocks,
            block_state,
        };
    }

    /// Create a new chunk of only air blocks; to be filled after creation.
    pub fn new_air(blocks: Vec<BlockId>, block_state: HashMap<usize, u16>) -> Self {
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

    pub fn set_block_state(&mut self, block_index: usize, state: u16) {
        self.block_state.insert(block_index, state);
    }

    pub fn remove_block_state(&mut self, block_index: &usize) {
        self.block_state.remove(&block_index);
    }

    pub fn get_block_state(&self, x: usize, y: usize, z: usize) -> Option<u16> {
        let index = x << 8 | y << 4 | z;
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
            return &self.blocks[idx[0] * CHUNK_SIZE.pow(2) + idx[1] * CHUNK_SIZE + idx[2]];
        }
    }
}

impl IndexMut<[usize; 3]> for Chunk {
    fn index_mut(&mut self, idx: [usize; 3]) -> &mut Self::Output {
        if self.is_uniform() {
            return &mut self.blocks[0];
        } else {
            return &mut self.blocks[idx[0] * CHUNK_SIZE.pow(2) + idx[1] * CHUNK_SIZE + idx[2]];
        }
    }
}

impl Index<IVec3> for Chunk {
    type Output = BlockId;

    fn index(&self, idx: IVec3) -> &Self::Output {
        if self.is_uniform() {
            return &self.blocks[0];
        } else {
            return &self.blocks[idx.x as usize * CHUNK_SIZE.pow(2)
                + idx.y as usize * CHUNK_SIZE
                + idx.z as usize];
        }
    }
}

impl IndexMut<IVec3> for Chunk {
    fn index_mut(&mut self, idx: IVec3) -> &mut Self::Output {
        if self.is_uniform() {
            return &mut self.blocks[0];
        } else {
            return &mut self.blocks[idx.x as usize * CHUNK_SIZE.pow(2)
                + idx.y as usize * CHUNK_SIZE
                + idx.z as usize];
        }
    }
}

/// Lookup table for which sides are visible from which in a chunk.
// Only used by chunk_loading_and_frustum_culling_system
#[derive(Component, Debug)]
pub struct VisibleSides {
    sides: HashMap<Direction, HashSet<Direction>>,
}

impl VisibleSides {
    pub async fn new(chunk: Chunk) -> Self {
        let mut sides: HashMap<Direction, HashSet<Direction>> = HashMap::with_capacity(6);

        if chunk.is_uniform() {
            let blocks = Blocks::get();

            if blocks[&chunk[0]].is_transparent() {
                for side in [
                    Direction::Forward,
                    Direction::Back,
                    Direction::Right,
                    Direction::Left,
                    Direction::Up,
                    Direction::Down,
                    Direction::None,
                ] {
                    sides.insert(
                        side,
                        HashSet::from([
                            Direction::Forward,
                            Direction::Back,
                            Direction::Right,
                            Direction::Left,
                            Direction::Up,
                            Direction::Down,
                            Direction::None,
                        ]),
                    );
                }
            } else {
                for side in [
                    Direction::Forward,
                    Direction::Back,
                    Direction::Right,
                    Direction::Left,
                    Direction::Up,
                    Direction::Down,
                    Direction::None,
                ] {
                    sides.insert(side, HashSet::new());
                }
            }

            let visible_sides = Self { sides };
            return visible_sides;
        }

        for side in [
            Direction::Forward,
            Direction::Back,
            Direction::Right,
            Direction::Left,
            Direction::Up,
            Direction::Down,
            Direction::None,
        ] {
            sides.insert(side, HashSet::new());
        }

        let mut visible_sides = Self { sides };
        visible_sides.update(&chunk);

        return visible_sides;
    }

    // Checks visibility from one side to the other.
    pub fn is_visible(&self, from: &Direction, to: &Direction) -> bool {
        return self.get(from).get(to).is_some();
    }

    pub fn get(&self, dir: &Direction) -> &HashSet<Direction> {
        self.sides.get(dir).unwrap()
    }

    fn reset(&mut self) {
        for side in self.sides.values_mut() {
            side.clear();
        }
    }

    fn extend(&mut self, sides: HashSet<Direction>) {
        for side in sides.iter() {
            self.sides
                .get_mut(side)
                .unwrap()
                .extend(sides.clone().into_iter());
        }
    }

    // For each face of the chunk, test which of the other sides are viewable looking through that face.
    pub fn update(&mut self, chunk: &Chunk) {
        self.reset();

        // Create a hashmap of blocks the floodfill has visited.
        let mut visited: HashSet<IVec3> = HashSet::with_capacity(CHUNK_SIZE.pow(3));

        // Iterate over the outermost blocks of the chunk
        for i in 0i32..CHUNK_SIZE as i32 {
            for j in 0i32..CHUNK_SIZE as i32 {
                for k in (0i32..CHUNK_SIZE as i32).step_by(15) {
                    let front_back = IVec3::new(i, j, k);
                    let left_right = IVec3::new(k, i, j);
                    let top_bottom = IVec3::new(i, k, j);
                    if visited.contains(&front_back)
                        || visited.contains(&left_right)
                        || visited.contains(&top_bottom)
                    {
                        continue;
                    } else {
                        // front and back
                        self.extend(Self::find_visible_sides(chunk, &mut visited, front_back));
                        // left and
                        self.extend(Self::find_visible_sides(chunk, &mut visited, left_right));
                        // top and b
                        self.extend(Self::find_visible_sides(chunk, &mut visited, top_bottom));
                    }
                }
            }
        }
    }

    // TODO: Maybe there's some smart way to do this? There's probably some floodfill algorithm.
    //
    // Starting from the seed block it propagates outwards from the block's sides.
    // If there's air there, that block is added to the queue of blocks to propagate from.
    // This continues until all air blocks that are connected are added to "visited".
    // If a block that is added to the queue is outside the chunk, it means the side from which
    // the block protrudes is visible from the other sides that are marked visible.
    fn find_visible_sides(
        chunk: &Chunk,
        visited: &mut HashSet<IVec3>,
        seed_block: IVec3,
    ) -> HashSet<Direction> {
        let mut visible_sides = HashSet::from([Direction::None]);

        let mut block_queue = Vec::with_capacity(CHUNK_SIZE.pow(3));
        block_queue.push(seed_block);

        let blocks = Blocks::get();

        // TODO: Do mut pos here and remove clone lines below?
        while let Some(pos) = block_queue.pop() {
            let direction = Direction::convert_position(&pos);
            match direction {
                Direction::None => (),
                _ => {
                    visible_sides.insert(direction);
                    // Continue loop since it went outside chunk.
                    continue;
                }
            }

            // Push all sides of block to queue.
            if !visited.contains(&pos) && blocks[&chunk[pos]].is_transparent() {
                block_queue.push({
                    let mut pos = pos.clone();
                    pos.x -= 1;
                    pos
                });
                block_queue.push({
                    let mut pos = pos.clone();
                    pos.x += 1;
                    pos
                });
                block_queue.push({
                    let mut pos = pos.clone();
                    pos.y -= 1;
                    pos
                });
                block_queue.push({
                    let mut pos = pos.clone();
                    pos.y += 1;
                    pos
                });
                block_queue.push({
                    let mut pos = pos.clone();
                    pos.z -= 1;
                    pos
                });
                block_queue.push({
                    let mut pos = pos.clone();
                    pos.z += 1;
                    pos
                });

                visited.insert(pos);
            }
        }
        return visible_sides;
    }
}
