use std::{collections::HashMap, ops::Index, sync::Arc};

use bevy::{
    prelude::*,
    render::{
        mesh::Indices, primitives::Aabb, render_resource::PrimitiveTopology, view::NoFrustumCulling,
    },
    tasks::{AsyncComputeTaskPool, Task},
};

use fmc_networking::{BlockId, NetworkClient};
use futures_lite::future;

use crate::{
    constants::*,
    game_state::GameState,
    rendering::materials,
    world::{
        blocks::{Block, BlockFace, BlockRotation, BlockState, Blocks, QuadPrimitive},
        world_map::{Chunk, ChunkMarker, WorldMap},
        Origin,
    },
};

use super::lighting::{Light, LightChunk, LightMap};

const TRIANGLES: [u32; 6] = [0, 1, 2, 2, 1, 3];

pub struct ChunkPlugin;

impl Plugin for ChunkPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<ChunkMeshEvent>();
        app.add_systems(
            Update,
            (mesh_system, handle_mesh_tasks).run_if(in_state(GameState::Playing)),
        );
    }
}

// Sent whenever we want to redraw a chunk
pub struct ChunkMeshEvent {
    /// Position of the chunk.
    pub position: IVec3,
}

/// Added to chunks that should be rendered.
#[derive(Component)]
pub struct MeshedChunkMarker;

#[derive(Component)]
pub struct ChunkMeshTask(
    Task<(
        Vec<(Handle<materials::BlockMaterial>, Mesh)>,
        Vec<(Handle<Scene>, Transform)>,
    )>,
);

/// Launches new mesh tasks when chunks change.
fn mesh_system(
    origin: Res<Origin>,
    mut commands: Commands,
    world_map: Res<WorldMap>,
    light_map: Res<LightMap>,
    mut mesh_events: EventReader<ChunkMeshEvent>,
    meshable_chunks: Query<With<MeshedChunkMarker>>,
    new_meshed_chunks: Query<&GlobalTransform, Added<MeshedChunkMarker>>,
) {
    let thread_pool = AsyncComputeTaskPool::get();

    for chunk_position in mesh_events.iter().map(|event| event.position).chain(
        new_meshed_chunks
            .iter()
            .map(|global| global.compute_transform().translation.as_ivec3() + origin.0),
    ) {
        match world_map.get_chunk(&chunk_position) {
            Some(chunk) => {
                if chunk.entity.is_some() && meshable_chunks.get(chunk.entity.unwrap()).is_ok() {
                    let expanded_chunk = world_map.get_expanded_chunk(chunk_position);
                    let expanded_light_chunk = light_map.get_expanded_chunk(chunk_position);

                    let task = thread_pool.spawn(build_mesh(expanded_chunk, expanded_light_chunk));
                    commands
                        .entity(chunk.entity.unwrap())
                        .insert(ChunkMeshTask(task));
                }
            }
            None => {
                //panic!("Tried to render a non-existing chunk.");
            }
        }
    }
}

// TODO: Some meshes are randomly offset by CHUNK_SIZE in any direction. I assume this is a bug
// with how the origin is handled, some race condition or some such that has nothing to do with
// rendering. The chunk probably arrives at the same tick origin is changed and gets messed up.
//
/// Meshes are computed async, this handles completed meshes
fn handle_mesh_tasks(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    scenes: Res<Assets<Scene>>,
    mut chunk_meshes: Query<(Entity, &mut ChunkMeshTask)>,
) {
    for (entity, mut task) in chunk_meshes.iter_mut() {
        if let Some((block_meshes, block_models)) = future::block_on(future::poll_once(&mut task.0))
        {
            let mut children = Vec::with_capacity(block_meshes.len() + block_models.len());

            for (material_handle, mesh) in block_meshes.into_iter() {
                children.push(
                    commands
                        .spawn(MaterialMeshBundle {
                            mesh: meshes.add(mesh.clone()),
                            material: material_handle.clone(),
                            ..Default::default()
                        })
                        // This is a marker for bevy's internal frustum culling, we do our own for
                        // chunk meshes.
                        .insert(NoFrustumCulling)
                        .id(),
                );
            }

            for (mut handle, transform) in block_models.into_iter() {
                handle.make_strong(&scenes);
                children.push(
                    commands
                        .spawn(SceneBundle {
                            scene: handle,
                            transform,
                            ..default()
                        })
                        .insert(NoFrustumCulling)
                        .id(),
                );
            }

            // Remove the previous meshes of the chunk
            commands.entity(entity).despawn_descendants();
            commands
                .entity(entity)
                .insert(VisibilityBundle::default())
                .remove::<ChunkMeshTask>()
                .push_children(&children);
        }
    }
}

/// Used to build a block mesh
#[derive(Default)]
struct MeshBuilder {
    pub vertices: Vec<[f32; 3]>,
    pub triangles: Vec<u32>,
    pub normals: Vec<[f32; 3]>,
    pub packed_bits: Vec<u32>,
    //pub texture_indices: Vec<i32>,
    pub face_count: u32,
}

impl MeshBuilder {
    fn to_mesh(self) -> Mesh {
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList);
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, self.vertices);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals);
        mesh.insert_attribute(materials::ATTRIBUTE_PACKED_BITS_0, self.packed_bits);

        mesh.set_indices(Some(Indices::U32(self.triangles)));
        return mesh;
    }

    fn add_face(
        &mut self,
        position: [f32; 3],
        quad: &QuadPrimitive,
        light: Light,
        block_state: BlockState,
        cull_delimiter: Option<(f32, f32)>,
    ) {
        let rotation = block_state.rotation();
        let mut vertices = quad.vertices.clone();

        if let Some((top_left, top_right)) = cull_delimiter {
            if vertices[0][1] <= top_left && vertices[2][1] <= top_right {
                return;
            }
            vertices[1][1] = vertices[1][1].max(top_left);
            vertices[3][1] = vertices[3][1].max(top_right);
        }

        for (i, mut vertex) in vertices.into_iter().enumerate() {
            if rotation != BlockRotation::None {
                rotation.rotate_vertex(&mut vertex);
            }

            vertex[0] += position[0];
            vertex[1] += position[1];
            vertex[2] += position[2];
            self.vertices.push(vertex);
            self.normals.push(quad.normals[i / 2]);
            // Pack bits, from right to left:
            // 19 bits, texture index
            // 3 bits, uv, 1 bit for if it should be diagonal, 2 for coordinate index
            // 5 bits, light, 1 bit bool true if sunlight, 4 bits intensity
            // TODO: Maybe better to rotate the vertices in mesh instead of shader? Possible way of
            // reclaiming bits if needed.
            // 3 bits, rotation, 1 bit upside down, 2 bit rotation around y axis
            self.packed_bits.push(
                quad.texture_array_id
                    | (i as u32) << 19
                    | (quad.rotate_texture as u32) << 21
                    | (light.0 as u32) << 22
                    | (rotation as u32) << 27,
            )
        }
        self.triangles
            .extend(TRIANGLES.iter().map(|x| x + 4 * self.face_count));
        self.face_count += 1;
    }
}

async fn build_mesh(
    chunk: ExpandedChunk,
    light_chunk: ExpandedLightChunk,
) -> (
    // all blocks of same material combined into same mesh
    Vec<(Handle<materials::BlockMaterial>, Mesh)>,
    // all blocks that use models to render, weak handles
    Vec<(Handle<Scene>, Transform)>,
) {
    let mut mesh_builders = HashMap::new();
    let mut scene_bundles = Vec::new();

    let blocks = Blocks::get();

    for x in 1..CHUNK_SIZE + 1 {
        for y in 1..CHUNK_SIZE + 1 {
            for z in 1..CHUNK_SIZE + 1 {
                let block_id = chunk.get_block(x, y, z).unwrap();

                let block_config = &blocks[&block_id];

                let block_state = if block_config.can_have_block_state() {
                    chunk
                        .get_block_state(x, y, z)
                        .unwrap_or(BlockState::default())
                } else {
                    BlockState::default()
                };

                match block_config {
                    Block::Cube(cube) => {
                        let builder =
                            if let Some(builder) = mesh_builders.get_mut(&cube.material_handle) {
                                builder
                            } else {
                                mesh_builders
                                    .insert(cube.material_handle.clone(), MeshBuilder::default());
                                mesh_builders.get_mut(&cube.material_handle).unwrap()
                            };

                        for quad in &cube.quads {
                            let cull_delimiter = if let Some(cull_face) = quad.cull_face {
                                let cull_face = cull_face.rotate(block_state.rotation());

                                let (x, y, z) = match cull_face {
                                    BlockFace::Back => (x, y, z - 1),
                                    BlockFace::Front => (x, y, z + 1),
                                    BlockFace::Bottom => (x, y - 1, z),
                                    BlockFace::Top => (x, y + 1, z),
                                    BlockFace::Left => (x - 1, y, z),
                                    BlockFace::Right => (x + 1, y, z),
                                };
                                let adjacent_block_id = match chunk.get_block(x, y, z) {
                                    Some(b) => b,
                                    None => continue,
                                };

                                let adjacent_block_config = &blocks[&adjacent_block_id];

                                if adjacent_block_config.culls(block_config) {
                                    let adjacent_block_state =
                                        if adjacent_block_config.can_have_block_state() {
                                            chunk
                                                .get_block_state(x, y, z)
                                                .unwrap_or(BlockState::default())
                                        } else {
                                            BlockState::default()
                                        };

                                    match adjacent_block_config.cull_delimiter(
                                        cull_face
                                            .invert()
                                            .reverse_rotate(adjacent_block_state.rotation()),
                                    ) {
                                        Some(deli) => Some(deli),
                                        None => continue,
                                    }
                                } else {
                                    None
                                }
                            } else {
                                None
                            };

                            // TODO: Water surfaces under solid blocks will be 0 light level. Quads
                            // need to be able to set the offset directly so that they can take
                            // the light level of the block they are from perhaps.
                            let light = match quad.light_face.rotate(block_state.rotation()) {
                                BlockFace::Right => light_chunk.get_light(x + 1, y, z),
                                BlockFace::Left => light_chunk.get_light(x - 1, y, z),
                                BlockFace::Front => light_chunk.get_light(x, y, z + 1),
                                BlockFace::Back => light_chunk.get_light(x, y, z - 1),
                                BlockFace::Top => light_chunk.get_light(x, y + 1, z),
                                BlockFace::Bottom => light_chunk.get_light(x, y - 1, z),
                            };

                            builder.add_face(
                                [x as f32 - 1.0, y as f32 - 1.0, z as f32 - 1.0],
                                quad,
                                light,
                                block_state,
                                cull_delimiter,
                            );
                        }
                    }
                    Block::Model(model) => {
                        let (handle, mut transform) = if block_state.uses_side_model() {
                            match &model.side {
                                Some((handle, transform)) => {
                                    (handle.clone_weak(), transform.clone())
                                }
                                None => panic!("Block state should have been validated at reception of the chunk.")
                            }
                        } else {
                            match &model.center {
                                Some((handle, transform)) => {
                                    (handle.clone_weak(), transform.clone())
                                }
                                None => panic!("Block state should have been validated at reception of the chunk.")
                            }
                        };

                        let mut rotation = match block_state.rotation() {
                            BlockRotation::None => Quat::from_rotation_y(0.0),
                            BlockRotation::Once => Quat::from_rotation_y(90.0),
                            BlockRotation::Twice => Quat::from_rotation_y(180.0),
                            BlockRotation::Thrice => Quat::from_rotation_y(270.0),
                        };

                        if block_state.is_upside_down() {
                            rotation *= Quat::from_rotation_x(180.0);
                        }

                        transform.rotate_around(Vec3::splat(0.5), rotation);
                        transform.translation += Vec3::new(x as f32, y as f32, z as f32) - 1.0;

                        scene_bundles.push((handle, transform));
                    }
                }
            }
        }
    }

    let meshes = mesh_builders
        .into_iter()
        .filter_map(|(material, mesh_builder)| {
            if mesh_builder.face_count == 0 {
                None
            } else {
                Some((material, mesh_builder.to_mesh()))
            }
        })
        .collect();

    return (meshes, scene_bundles);
}

// TODO: This used to used to store 2d arrays for the surrounding chunks, but changed to Chunk's to
// have access to block state while rendering. After changing though it looks to me like it renders
// slower (not actually sure). How can this be? Constructing the arrays must surely be way more
// expensive! Maybe it's because of having to map the option every time it's accessing a block.
// Might be worth testing just storing the blocks as a vec instead of the Chunk struct, empty
// vecs for chunks that don't exist.
// See commit 'b5d40b1' for array layout
//
/// Larger chunk containing both the chunks and the immediate blocks around it.
pub struct ExpandedChunk {
    pub center: Chunk,
    pub top: Option<Chunk>,
    pub bottom: Option<Chunk>,
    pub right: Option<Chunk>,
    pub left: Option<Chunk>,
    pub front: Option<Chunk>,
    pub back: Option<Chunk>,
}

impl ExpandedChunk {
    fn get_block(&self, x: usize, y: usize, z: usize) -> Option<BlockId> {
        if x == 0 {
            return self.left.as_ref().map(|chunk| chunk[[15, y - 1, z - 1]]);
        } else if x == 17 {
            return self.right.as_ref().map(|chunk| chunk[[0, y - 1, z - 1]]);
        } else if y == 0 {
            return self.bottom.as_ref().map(|chunk| chunk[[x - 1, 15, z - 1]]);
        } else if y == 17 {
            return self.top.as_ref().map(|chunk| chunk[[x - 1, 0, z - 1]]);
        } else if z == 0 {
            return self.back.as_ref().map(|chunk| chunk[[x - 1, y - 1, 15]]);
        } else if z == 17 {
            return self.front.as_ref().map(|chunk| chunk[[x - 1, y - 1, 0]]);
        } else {
            return Some(self.center[[x - 1, y - 1, z - 1]]);
        }
    }

    fn get_block_state(&self, x: usize, y: usize, z: usize) -> Option<BlockState> {
        if x == 0 {
            return self
                .left
                .as_ref()
                .and_then(|chunk| chunk.get_block_state(15, y - 1, z - 1));
        } else if x == 17 {
            return self
                .right
                .as_ref()
                .and_then(|chunk| chunk.get_block_state(0, y - 1, z - 1));
        } else if y == 0 {
            return self
                .bottom
                .as_ref()
                .and_then(|chunk| chunk.get_block_state(x - 1, 15, z - 1));
        } else if y == 17 {
            return self
                .top
                .as_ref()
                .and_then(|chunk| chunk.get_block_state(x - 1, 0, z - 1));
        } else if z == 0 {
            return self
                .back
                .as_ref()
                .and_then(|chunk| chunk.get_block_state(x - 1, y - 1, 15));
        } else if z == 17 {
            return self
                .front
                .as_ref()
                .and_then(|chunk| chunk.get_block_state(x - 1, y - 1, 0));
        } else {
            return self.center.get_block_state(x - 1, y - 1, z - 1);
        }
    }
}

pub struct ExpandedLightChunk {
    pub center: LightChunk,
    pub top: [[Light; CHUNK_SIZE]; CHUNK_SIZE],
    pub bottom: [[Light; CHUNK_SIZE]; CHUNK_SIZE],
    pub right: [[Light; CHUNK_SIZE]; CHUNK_SIZE],
    pub left: [[Light; CHUNK_SIZE]; CHUNK_SIZE],
    pub front: [[Light; CHUNK_SIZE]; CHUNK_SIZE],
    pub back: [[Light; CHUNK_SIZE]; CHUNK_SIZE],
}

impl ExpandedLightChunk {
    fn get_light(&self, x: usize, y: usize, z: usize) -> Light {
        if x == 0 {
            return self.left[y - 1][z - 1];
        } else if x == 17 {
            return self.right[y - 1][z - 1];
        } else if y == 0 {
            return self.bottom[x - 1][z - 1];
        } else if y == 17 {
            return self.top[x - 1][z - 1];
        } else if z == 0 {
            return self.back[x - 1][y - 1];
        } else if z == 17 {
            return self.front[x - 1][y - 1];
        } else {
            return self.center[[x - 1, y - 1, z - 1]];
        }
    }
}
