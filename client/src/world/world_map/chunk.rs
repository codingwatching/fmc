use std::collections::{HashMap, HashSet};
use std::ops::{Index, IndexMut};
use std::slice::Iter;

use bevy::{
    prelude::*,
    tasks::{AsyncComputeTaskPool, Task},
};
use fmc_networking::BlockId;
use futures_lite::future;

use crate::utils;
use crate::world::blocks::{BlockState, Blocks};
use crate::{constants::*, game_state::GameState, world::world_map::WorldMap};

pub(super) struct ChunkPlugin;
impl Plugin for ChunkPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<ComputeVisibleChunkFacesEvent>();
        app.add_systems(
            Update,
            (handle_visibility_tasks, spawn_visiblity_tasks).run_if(GameState::in_game),
        );
    }
}

/// Event sent when the visible chunk faces of a chunk should be recomputed.
#[derive(Event)]
pub struct ComputeVisibleChunkFacesEvent(pub IVec3);

#[derive(Component)]
struct VisibleSidesTask(Task<VisibleChunkFaces>);

// TODO: Would be really nice to use bevy change detection to trigger this instead of events.
//       Dont' know how, maybe on Mesh change? But only has access to mesh handle.
/// Run whenever a chunk changes.
fn spawn_visiblity_tasks(
    mut commands: Commands,
    world_map: Res<WorldMap>,
    mut find_visible_sides_events: EventReader<ComputeVisibleChunkFacesEvent>,
) {
    let thread_pool = AsyncComputeTaskPool::get();

    for position in find_visible_sides_events.read() {
        if let Some(chunk) = world_map.get_chunk(&position.0) {
            if let Some(entity) = chunk.entity {
                let task = thread_pool.spawn(VisibleChunkFaces::new(chunk.clone()));
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

fn handle_visibility_tasks(
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
    /// XXX: Notice that the coordinates align with the rendering world, the z axis extends
    /// out of the screen. 0,0,0 is the bottom left FAR corner. Not bottom left NEAR.
    /// A CHUNK_SIZE^3 array containing all the blocks in the chunk.
    /// Indexed by x*CHUNK_SIZE^2 + z*CHUNK_SIZE + y
    blocks: Vec<BlockId>,
    /// Optional block state containing info like rotation and color
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

/// Lookup table for which sides are visible from which in a chunk.
// Only used by chunk_loading_and_frustum_culling_system
#[derive(Component, Debug)]
pub struct VisibleChunkFaces {
    faces: HashMap<ChunkFace, HashSet<ChunkFace>>,
}

impl VisibleChunkFaces {
    pub async fn new(chunk: Chunk) -> Self {
        let mut faces: HashMap<ChunkFace, HashSet<ChunkFace>> = HashMap::with_capacity(6);

        if chunk.is_uniform() {
            let blocks = Blocks::get();

            if blocks[&chunk[0]].is_transparent() {
                for chunk_face in [
                    ChunkFace::Front,
                    ChunkFace::Back,
                    ChunkFace::Right,
                    ChunkFace::Left,
                    ChunkFace::Top,
                    ChunkFace::Bottom,
                    ChunkFace::None,
                ] {
                    faces.insert(
                        chunk_face,
                        HashSet::from([
                            ChunkFace::Front,
                            ChunkFace::Back,
                            ChunkFace::Right,
                            ChunkFace::Left,
                            ChunkFace::Top,
                            ChunkFace::Bottom,
                            ChunkFace::None,
                        ]),
                    );
                }
            } else {
                for chunk_face in [
                    ChunkFace::Front,
                    ChunkFace::Back,
                    ChunkFace::Right,
                    ChunkFace::Left,
                    ChunkFace::Top,
                    ChunkFace::Bottom,
                    ChunkFace::None,
                ] {
                    faces.insert(chunk_face, HashSet::new());
                }
            }

            let visible_chunk_faces = Self { faces };
            return visible_chunk_faces;
        }

        for chunk_face in [
            ChunkFace::Front,
            ChunkFace::Back,
            ChunkFace::Right,
            ChunkFace::Left,
            ChunkFace::Top,
            ChunkFace::Bottom,
            ChunkFace::None,
        ] {
            faces.insert(chunk_face, HashSet::new());
        }

        let mut visible_chunk_faces = Self { faces };
        visible_chunk_faces.update(&chunk);

        return visible_chunk_faces;
    }

    // Checks visibility from one side to the other.
    pub fn is_visible(&self, from: &ChunkFace, to: &ChunkFace) -> bool {
        return self.get(from).get(to).is_some();
    }

    pub fn get(&self, dir: &ChunkFace) -> &HashSet<ChunkFace> {
        self.faces.get(dir).unwrap()
    }

    fn reset(&mut self) {
        for side in self.faces.values_mut() {
            side.clear();
        }
    }

    fn extend(&mut self, chunk_faces: HashSet<ChunkFace>) {
        for face in chunk_faces.iter() {
            self.faces
                .get_mut(face)
                .unwrap()
                .extend(chunk_faces.clone().into_iter());
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
                for k in (0i32..CHUNK_SIZE as i32).step_by(CHUNK_SIZE - 1) {
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
                        self.extend(Self::find_visible_chunk_faces(
                            chunk,
                            &mut visited,
                            front_back,
                        ));
                        // left and
                        self.extend(Self::find_visible_chunk_faces(
                            chunk,
                            &mut visited,
                            left_right,
                        ));
                        // top and b
                        self.extend(Self::find_visible_chunk_faces(
                            chunk,
                            &mut visited,
                            top_bottom,
                        ));
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
    fn find_visible_chunk_faces(
        chunk: &Chunk,
        visited: &mut HashSet<IVec3>,
        seed_block: IVec3,
    ) -> HashSet<ChunkFace> {
        let mut visible_chunk_faces = HashSet::from([ChunkFace::None]);

        let mut block_queue = Vec::with_capacity(CHUNK_SIZE.pow(3));
        block_queue.push(seed_block);

        let blocks = Blocks::get();

        // TODO: Do mut pos here and remove clone lines below?
        while let Some(pos) = block_queue.pop() {
            let direction = ChunkFace::convert_position(&pos);
            match direction {
                ChunkFace::None => (),
                _ => {
                    visible_chunk_faces.insert(direction);
                    // Continue loop since it went outside chunk.
                    continue;
                }
            }

            // Push all faces of block to queue.
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
        return visible_chunk_faces;
    }
}

#[derive(Debug, PartialEq, Eq, Hash, Clone)]
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
    pub fn convert_position(pos: &IVec3) -> Self {
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
