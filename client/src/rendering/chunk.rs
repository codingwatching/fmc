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
        blocks::{Block, BlockFace, Blocks, QuadPrimitive},
        world_map::{Chunk, ChunkMarker, ChunkRequestEvent, WorldMap},
    },
};

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
    /// Flag for when the chunk should not have a mesh created unless one already
    /// exists.
    pub should_create: bool,
}

/// Marker struct for entities that are meshes of chunks.
#[derive(Component)]
pub struct ChunkMeshMarker;

#[derive(Component)]
pub struct ChunkMeshTask(
    Task<(
        Vec<(Handle<materials::BlockMaterial>, Mesh)>,
        Vec<(Handle<Scene>, Transform)>,
    )>,
);

/// Launches new mesh tasks when chunks change.
fn mesh_system(
    mut commands: Commands,
    world_map: Res<WorldMap>,
    mut chunk_request_events: EventWriter<ChunkRequestEvent>,
    mut mesh_events: EventReader<ChunkMeshEvent>,
    mesh_task_query: Query<(With<ChunkMarker>, With<Visibility>)>,
) {
    let thread_pool = AsyncComputeTaskPool::get();

    for event in mesh_events.iter() {
        match world_map.get_chunk(&event.position) {
            Some(chunk) => {
                if event.should_create || mesh_task_query.get(chunk.entity.unwrap()).is_ok() {
                    let expanded_chunk = match world_map.get_expanded_chunk(event.position) {
                        Ok(e) => e,
                        Err(needed) => {
                            chunk_request_events.send_batch(needed);
                            continue;
                        }
                    };

                    let task = thread_pool.spawn(build_mesh(expanded_chunk));
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
                        .insert(ChunkMeshMarker)
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
    //pub triangles: Vec<u32>,
    //pub normals: Vec<[f32; 3]>,
    //pub uvs: Vec<u32>,
    pub uvs: Vec<[f32; 2]>,
    pub texture_indices: Vec<i32>,
    pub face_count: u32,
}

impl MeshBuilder {
    fn to_mesh(self) -> Mesh {
        let mut mesh = Mesh::new(PrimitiveTopology::TriangleList);
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, self.vertices);
        //mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals);
        //mesh.insert_attribute(materials::BLOCK_ATTRIBUTE_UV, self.uvs);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, self.uvs);
        mesh.insert_attribute(materials::BLOCK_ATTRIBUTE_UV, self.texture_indices);

        mesh.compute_flat_normals();
        //mesh.generate_tangents().unwrap();
        // TODO: Remove these if it saves memory.
        //mesh.set_indices(Some(Indices::U32(self.triangles)));
        return mesh;
    }

    fn add_face(&mut self, position: [f32; 3], quad: &QuadPrimitive) {
        //for (i, vertex) in quad.vertices.iter().enumerate() {
        //    self.vertices
        //        .push([vertex[0] + position[0], vertex[1] + position[1], vertex[2] + position[2]]);
        //    self.normals.extend(&quad.normals);
        //    //self.normals.push(quad.normal);
        //}
        self.vertices.push([
            quad.vertices[0][0] + position[0],
            quad.vertices[0][1] + position[1],
            quad.vertices[0][2] + position[2],
        ]);
        self.vertices.push([
            quad.vertices[1][0] + position[0],
            quad.vertices[1][1] + position[1],
            quad.vertices[1][2] + position[2],
        ]);
        self.vertices.push([
            quad.vertices[2][0] + position[0],
            quad.vertices[2][1] + position[1],
            quad.vertices[2][2] + position[2],
        ]);
        self.vertices.push([
            quad.vertices[2][0] + position[0],
            quad.vertices[2][1] + position[1],
            quad.vertices[2][2] + position[2],
        ]);
        self.vertices.push([
            quad.vertices[1][0] + position[0],
            quad.vertices[1][1] + position[1],
            quad.vertices[1][2] + position[2],
        ]);
        self.vertices.push([
            quad.vertices[3][0] + position[0],
            quad.vertices[3][1] + position[1],
            quad.vertices[3][2] + position[2],
        ]);
        //self.triangles.extend(
        //    TRIANGLES
        //        .iter()
        //        .map(|x| x + 4 * self.face_count)
        //);

        const UVS: [[f32; 2]; 6] = [
            [0.0, 1.0],
            [0.0, 0.0],
            [1.0, 1.0],
            [1.0, 1.0],
            [0.0, 0.0],
            [1.0, 0.0],
        ];

        // TODO: This packing was premature.
        for i in 0..6 {
            //self.normals.push(quad.normal);
            // Pack bits, first 2 bits are uv, last 19 bits are texture_index
            //self.uvs.push((i << 29) | quad.texture_array_id)
            self.uvs.push(UVS[i]);
            self.texture_indices.push(quad.texture_array_id as i32);
        }
        self.face_count += 1;
    }
}

// TODO: Implemented simple version first that takes up ~2gb of memory for a 32x world.
// Both normals and uvs should be packed into the vertex. To do this it needs to be able to
// separate blocks that are cubes and those which aren't. Cubes only have 6 normals so they can be
// packed in 3 bits.
async fn build_mesh(
    expanded_chunk: ExpandedChunk,
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
                let block_id = expanded_chunk[[x, y, z]];

                let block_config = &blocks[&block_id];

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
                            if let Some(cull_face) = quad.cull_face {
                                let adjacent_block_id = match cull_face {
                                    // Mesh gets culled by front face, so we get the block behind,
                                    // back, get front etc.
                                    BlockFace::Front => expanded_chunk[[x, y, z + 1]],
                                    BlockFace::Back => expanded_chunk[[x, y, z - 1]],
                                    BlockFace::Bottom => expanded_chunk[[x, y + 1, z]],
                                    BlockFace::Top => expanded_chunk[[x, y - 1, z]],
                                    BlockFace::Left => expanded_chunk[[x + 1, y, z]],
                                    BlockFace::Right => expanded_chunk[[x - 1, y, z]],
                                };

                                let adjacent_block_config = &blocks[&adjacent_block_id];

                                if adjacent_block_config.only_cull_if_same()
                                    && block_id == adjacent_block_id
                                {
                                    continue;
                                } else if adjacent_block_config.culls_face(cull_face) {
                                    continue;
                                }
                            }

                            builder
                                .add_face([x as f32 - 1.0, y as f32 - 1.0, z as f32 - 1.0], quad);
                        }
                    }
                    Block::Model(model) => {
                        let rotation = match expanded_chunk.get_block_state(x, y, z) {
                            Some(b) => b,
                            None => panic!(
                                "Block state should have been validated at reception of the chunk."
                            ),
                        };

                        let (handle, mut transform) = if rotation & 0b0100 != 0 {
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

                        match rotation & 0b0011 {
                            // north, default
                            0 => (),
                            // east
                            1 => transform.rotate_local_y(90.0),
                            // south
                            2 => transform.rotate_local_y(180.0),
                            // west
                            3 => transform.rotate_local_y(270.0),
                            _ => unreachable!(),
                        }

                        if rotation & 0b1000 == 0b1000 {
                            transform.rotate_local_x(180.0)
                        }

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

/// Larger chunk containing both the chunks and the immediate blocks around it.
pub struct ExpandedChunk {
    pub center: Chunk,
    pub top: [[BlockId; CHUNK_SIZE]; CHUNK_SIZE],
    pub bottom: [[BlockId; CHUNK_SIZE]; CHUNK_SIZE],
    pub right: [[BlockId; CHUNK_SIZE]; CHUNK_SIZE],
    pub left: [[BlockId; CHUNK_SIZE]; CHUNK_SIZE],
    pub front: [[BlockId; CHUNK_SIZE]; CHUNK_SIZE],
    pub back: [[BlockId; CHUNK_SIZE]; CHUNK_SIZE],
}

impl ExpandedChunk {
    fn get_block_state(&self, x: usize, y: usize, z: usize) -> Option<u16> {
        return self.center.get_block_state(x - 1, y - 1, z - 1);
    }
}

impl Index<[usize; 3]> for ExpandedChunk {
    type Output = BlockId;

    fn index(&self, index: [usize; 3]) -> &Self::Output {
        if index[0] == 0 {
            return &self.left[index[1] - 1][index[2] - 1];
        } else if index[0] == 17 {
            return &self.right[index[1] - 1][index[2] - 1];
        } else if index[1] == 0 {
            return &self.bottom[index[0] - 1][index[2] - 1];
        } else if index[1] == 17 {
            return &self.top[index[0] - 1][index[2] - 1];
        } else if index[2] == 0 {
            return &self.back[index[0] - 1][index[1] - 1];
        } else if index[2] == 17 {
            return &self.front[index[0] - 1][index[1] - 1];
        } else {
            return &self.center[[index[0] - 1, index[1] - 1, index[2] - 1]];
        }
    }
}
