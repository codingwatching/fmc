use crate::{
    constants::*,
    game_state::GameState,
    player::Player,
    rendering::chunk::MeshedChunkMarker,
    settings, utils,
    world::{
        blocks::{Block, BlockState, Blocks},
        world_map::{
            chunk::{
                Chunk, ChunkFace, ChunkMarker, ComputeVisibleChunkFacesEvent, VisibleChunkFaces,
            },
            WorldMap,
        },
        MovesWithOrigin, Origin,
    },
};

use bevy::{
    prelude::*,
    render::primitives::{Frustum, Sphere},
};
use fmc_networking::{messages, NetworkClient, NetworkData};
use std::collections::{HashMap, HashSet};

/// Keeps track of which chunks should be loaded/unloaded.
pub struct ChunkManagerPlugin;
impl Plugin for ChunkManagerPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<ChunkRequestEvent>()
            .init_resource::<Requested>()
            .init_resource::<Pause>()
            .add_systems(
                Update,
                (
                    // TODO: I wish these didn't need to be ordered.
                    // Currently it has to send before it handles, so it doesn't request a chunk
                    // that has already been received.
                    // It has to handle before the loading functions as they will insert request
                    // events for chunks that have already been received.
                    send_chunk_requests,
                    handle_chunk_responses.after(send_chunk_requests),
                    frustum_chunk_loading.after(handle_chunk_responses),
                    proximity_chunk_loading.after(handle_chunk_responses),
                    handle_block_updates,
                    pause_system,
                )
                    .run_if(GameState::in_game),
            )
            .add_systems(
                // This has to be run postupdate because of the async mesh/visibility tasks. If
                // it despawns the same tick a task is finished it will panic on trying to insert
                // for non-existing entity. https://github.com/bevyengine/bevy/issues/3845 relevant
                // issue
                PostUpdate,
                unload_chunks.run_if(resource_changed::<Origin>()),
            );
    }
}

// A cache of currently requested chunks, to alleviate strain on the server.
// Otherwise the client would request the same chunk multiple times while it is in transit from the
// server.
#[derive(Resource, Default)]
struct Requested {
    pub chunks: HashSet<IVec3>,
}

#[derive(Resource, Default)]
struct Pause(bool);

// Event sent from systems that want to request a chunk from the server.
#[derive(Default, Event)]
pub struct ChunkRequestEvent(pub IVec3);

fn pause_system(mut pause: ResMut<Pause>, keyboard_input: Res<Input<KeyCode>>) {
    if keyboard_input.just_pressed(KeyCode::F5) {
        pause.0 = !pause.0;
    }
}

// Removes chunks that are outside the render distance of the player.
fn unload_chunks(
    origin: Res<Origin>,
    mut world_map: ResMut<WorldMap>,
    settings: Res<settings::Settings>,
    mut commands: Commands,
) {
    world_map.chunks.retain(|chunk_pos, chunk| {
        let distance = (*chunk_pos - origin.0).abs() / IVec3::splat(CHUNK_SIZE as i32);
        if distance
            .cmpgt(IVec3::splat(settings.render_distance as i32))
            .any()
        {
            if let Some(entity) = chunk.entity {
                commands.entity(entity).despawn_recursive();
            }
            false
        } else {
            true
        }
    });
}

// The frustum chunk loading system needs some help. This loads the 3x3x3 chunks that are closest.
// This is for when the player walks into a chunk without looking at it first. The player might
// also collide with these without having looked at them (or collide with a chunk that isn't
// actually visible)
fn proximity_chunk_loading(
    origin: Res<Origin>,
    world_map: Res<WorldMap>,
    player_position: Query<&GlobalTransform, With<Player>>,
    mut chunk_request_events: EventWriter<ChunkRequestEvent>,
    pause: Res<Pause>,
) {
    if pause.0 {
        return;
    }
    let player_position = player_position.single();
    let player_chunk_position = utils::world_position_to_chunk_pos(
        player_position.translation().floor().as_ivec3() + origin.0,
    );

    for x in (player_chunk_position.x - CHUNK_SIZE as i32
        ..player_chunk_position.x + CHUNK_SIZE as i32)
        .step_by(CHUNK_SIZE)
    {
        for y in (player_chunk_position.y - CHUNK_SIZE as i32
            ..player_chunk_position.y + CHUNK_SIZE as i32)
            .step_by(CHUNK_SIZE)
        {
            for z in (player_chunk_position.z - CHUNK_SIZE as i32
                ..player_chunk_position.z + CHUNK_SIZE as i32)
                .step_by(CHUNK_SIZE)
            {
                let position = IVec3::new(x, y, z);
                if !world_map.contains_chunk(&position) {
                    chunk_request_events.send(ChunkRequestEvent(position));
                }
            }
        }
    }
}

// TODO: If the below can be implemented I think it's possible it is cheap enough to be done server
// side. It would be better, as it would remove the ping pong where the client has to wait for a
// chunk to know if it should ask for adjacent chunks. It would also look much better as chunks
// would not need to be loaded when the player turns around, it would already have them. Chunk
// loading would also only have to be done whenever the player crosses a chunk border. Cleaner as
// well as chunk visibility and chunk loading would be completely separate, as it is now it looks
// very messy. AND it would be inherent anti-cheat.
// TODO: When implementing this I wanted to create chunk columns where all vertically adjacent air
// chunks belonged to the same column (with the first chunk with blocks below them as the column
// base). This would reduce the search drastically when at the surface as you could check entire
// columns in one step, instead of going through all their chunks individually. Didn't do it because
// it was too hard to imagine how it would work. Went with simpler version to save time. Maybe
// implement this or maybe ray tracing can solve it. Meanwhile, it will take up a huge chunk of the
// frame time.
fn frustum_chunk_loading(
    mut commands: Commands,
    origin: Res<Origin>,
    world_map: Res<WorldMap>,
    camera_query: Query<(&Frustum, &GlobalTransform), With<Camera>>,
    settings: Res<settings::Settings>,
    pause: Res<Pause>,
    mut chunk_request_events: EventWriter<ChunkRequestEvent>,
    mut chunk_query: Query<
        (
            Entity,
            &VisibleChunkFaces,
            Option<&mut Visibility>,
            Option<&MeshedChunkMarker>,
        ),
        With<ChunkMarker>,
    >,
) {
    if pause.0 {
        return;
    }

    // Reset the visibility of all chunks
    chunk_query.for_each_mut(|(_, _, visibility, _)| {
        if let Some(mut visibility) = visibility {
            *visibility = Visibility::Hidden;
        }
    });

    let (frustum, camera_position) = camera_query.single();

    let view_vector = camera_position.forward();

    // The order of search directions. forward -> surrounding -> remaining direction
    // Example:
    // forward     =      ---------- forward -----------
    //                   /         /         \          \
    // surrounding =   left      right       top      bottom
    //                 /  \      /  \       /  \       /  \
    // orthogonal  = [top,bot] [top,bot] [top,bot] [top, bottom]
    let forward_dir = ChunkFace::convert_vector(&view_vector);
    let surrounding = forward_dir.surrounding();
    let orthogonal = forward_dir.orthogonal(&surrounding);

    let mut forward_queue = Vec::with_capacity(settings.render_distance as usize);
    let mut surr_queue = Vec::with_capacity(settings.render_distance as usize);
    let mut ortho_queue = Vec::with_capacity(settings.render_distance.pow(3) as usize);

    let mut already_visited = HashSet::with_capacity(settings.render_distance.pow(3) as usize);

    // Goes one chunk in the direction specified, checks if it's visible and inside the frustum
    // before it adds it to the queue.
    let mut traverse_direction =
        |mut chunk_position: IVec3,
         from_face: &ChunkFace, // Direction to enter from
         to_face: &ChunkFace,   // Direction to exit through
         visible_chunk_faces: Option<&VisibleChunkFaces>,
         queue: &mut Vec<(IVec3, ChunkFace)>| {
            // If visible_chunk_faces is None it's an air chunk (all chunk faces are visible)
            // If from_face is not visible from to_face, this is where it stops.
            if let Some(visible_faces) = visible_chunk_faces {
                if !visible_faces.is_visible(from_face, to_face) {
                    return;
                }
            }

            chunk_position = to_face.shift_position(chunk_position);

            // Only visit a chunk once, no matter which side you enter from
            if already_visited.contains(&chunk_position) {
                return;
            }

            let distance = chunk_position - origin.0;

            // 0.867 is half the diagonal of a 1x1x1 cube, i.e the radius of the sphere around the
            // cube. It is scaled by the side length the chunk.
            if !frustum.intersects_sphere(
                &Sphere {
                    center: distance.as_vec3a() + CHUNK_SIZE as f32 / 2.0,
                    radius: 0.867 * CHUNK_SIZE as f32,
                },
                true,
            ) {
                return;
            }

            // TODO: Intersection with the far plane should have been done above, but the result is
            // wrong allowing chunks that are up to 3 and maybe more chunks past the far plane.
            // The plan is to remove the frustum from this calculation so I won't bother finding
            // what's wrong.
            if ((distance / IVec3::splat(CHUNK_SIZE as i32)).abs())
                // -2 is to create a buffer to the unloading border. Otherwise difference in
                // position on server and client will cause some chunks to be unlaoded prematurely.
                // Chunk loading will eventually be handled by the server, this is a quickfix.
                .cmpgt(IVec3::splat(settings.render_distance as i32 - 2))
                .any()
            {
                return;
            }

            already_visited.insert(chunk_position);
            queue.push((chunk_position, to_face.opposite()));
        };

    // Reads a chunk from the chunk map. If it does not exist, it requests it.
    // Returns Option<Option<VisibleChunkFaces>>, first option is if there is a chunk that can be
    // traversed there, the second is what kind of chunk it is.
    // Normal chunk => it has VisibleChunkFaces = Some(Some(VisibleChunkFaces))
    // uniform and transparent (probably air) => it doesn't have VisibleChunkFaces = Some(None)
    // Chunk that hasn't computed its VisibleChunkFaces yet => None(None) = None
    macro_rules! read_chunk {
        ($pos:expr) => {{
            if let Some(chunk) = world_map.get_chunk($pos) {
                if let Some(chunk_entity) = chunk.entity {
                    unsafe {
                        if let Ok((entity, visible_chunk_faces, visibility, is_meshed_chunk)) =
                            chunk_query.get_unchecked(chunk_entity)
                        {
                            if let Some(mut visibility) = visibility {
                                *visibility = Visibility::Inherited;
                            } else if is_meshed_chunk.is_none() {
                                commands.entity(entity).insert(MeshedChunkMarker);
                            }
                            Some(Some(visible_chunk_faces))
                        } else {
                            // visible_chunk_faces not finished yet
                            None
                        }
                    }
                } else {
                    // No entity means it is an air chunk.
                    Some(None)
                }
            } else {
                chunk_request_events.send(ChunkRequestEvent(*$pos));
                None
            }
        }};
    }

    // The player chunk serves as the tree root.
    let player_chunk_pos = utils::world_position_to_chunk_pos(
        camera_position.translation().floor().as_ivec3() + origin.0,
    );
    forward_queue.push((player_chunk_pos, ChunkFace::None));

    while let Some((forward_pos, forward_entry_face)) = forward_queue.pop() {
        let forward_faces = match read_chunk!(&forward_pos) {
            Some(faces) => faces,
            None => continue,
        };

        if forward_entry_face == ChunkFace::None {}

        for (i, surr_dir) in surrounding.iter().enumerate() {
            traverse_direction(
                forward_pos,
                &forward_entry_face,
                surr_dir,
                forward_faces,
                &mut surr_queue,
            );
            while let Some((surr_pos, surr_entry_face)) = surr_queue.pop() {
                let surr_faces = match read_chunk!(&surr_pos) {
                    Some(faces) => faces,
                    None => continue,
                };
                for ortho_dir in orthogonal[i].iter() {
                    traverse_direction(
                        surr_pos,
                        &surr_entry_face,
                        ortho_dir,
                        surr_faces,
                        &mut ortho_queue,
                    );
                    while let Some((ortho_pos, ortho_entry_face)) = ortho_queue.pop() {
                        let ortho_faces = match read_chunk!(&ortho_pos) {
                            Some(faces) => faces,
                            None => continue,
                        };
                        // XXX: This ordering is most likely not correct and might cause visibility
                        // bugs. Complexity too much for me.
                        traverse_direction(
                            ortho_pos,
                            &ortho_entry_face,
                            &forward_dir,
                            ortho_faces,
                            &mut ortho_queue,
                        );
                        traverse_direction(
                            ortho_pos,
                            &ortho_entry_face,
                            surr_dir,
                            ortho_faces,
                            &mut ortho_queue,
                        );
                        traverse_direction(
                            ortho_pos,
                            &ortho_entry_face,
                            ortho_dir,
                            ortho_faces,
                            &mut ortho_queue,
                        );
                    }
                }
                traverse_direction(
                    surr_pos,
                    &surr_entry_face,
                    &forward_dir,
                    surr_faces,
                    &mut surr_queue,
                );
                traverse_direction(
                    surr_pos,
                    &surr_entry_face,
                    surr_dir,
                    surr_faces,
                    &mut surr_queue,
                );
            }
        }

        traverse_direction(
            forward_pos,
            &forward_entry_face,
            &forward_dir,
            forward_faces,
            &mut forward_queue,
        );
    }
}

/// Sends chunk requests to the server.
fn send_chunk_requests(
    //origin: Res<Origin>,
    mut requested: ResMut<Requested>,
    mut request_events: EventReader<ChunkRequestEvent>,
    net: Res<NetworkClient>,
) {
    let mut chunk_request = messages::ChunkRequest::new();
    for chunk_pos in request_events.read() {
        if !requested.chunks.contains(&chunk_pos.0) {
            requested.chunks.insert(chunk_pos.0);
            chunk_request.chunks.push(chunk_pos.0);
        }
    }
    if !chunk_request.chunks.is_empty() {
        net.send_message(chunk_request);
    }
}

// TODO: This could take like ResMut<Events<ChunkResponse>> and drain the chunks to avoid
// reallocation. The lighting system listens for the same event, and it is nice to have the systems
// self-contained. Maybe the world map should contain only the chunk entity. This way There would
// no longer be a need for ComputeVisibleChunkFacesEvent either. Everything just listens for
// Changed<Chunk>. Accessing the world_map isn't actually a bottleneck I think, and doing a double
// lookup can't be that bad.
//
/// Handles chunks sent from the server.
fn handle_chunk_responses(
    origin: Res<Origin>,
    mut commands: Commands,
    mut world_map: ResMut<WorldMap>,
    mut requested: ResMut<Requested>,
    net: Res<NetworkClient>,
    //mut chunk_responses: ResMut<Events<NetworkData<messages::ChunkResponse>>>,
    mut chunk_responses: EventReader<NetworkData<messages::ChunkResponse>>,
    mut visibility_task_events: EventWriter<ComputeVisibleChunkFacesEvent>,
    //mut amount: Local<usize>, //chunk_query: Query<(&Chunk, &chunk::Blocks)>,
) {
    for response in chunk_responses.read() {
        let blocks = Blocks::get();

        for chunk in response.chunks.iter() {
            // TODO: Need to validate block state too. Server can crash client.
            for block_id in chunk.blocks.iter() {
                if !blocks.contain(*block_id) {
                    net.disconnect(format!(
                        "Server sent chunk with unknown block id: '{}'",
                        block_id
                    ));
                    return;
                }
            }

            if chunk.blocks.len() == 1
                && match &blocks[&chunk.blocks[0]] {
                    Block::Cube(b) if b.quads.len() == 0 => true,
                    _ => false,
                }
            {
                world_map.insert(
                    chunk.position,
                    Chunk::new_air(
                        chunk.blocks.clone(),
                        chunk
                            .block_state
                            .iter()
                            .map(|(&k, &v)| (k, BlockState(v)))
                            .collect(),
                    ),
                );
            } else {
                let entity = commands
                    .spawn(TransformBundle {
                        local: Transform::from_translation((chunk.position - origin.0).as_vec3()),
                        ..default()
                    })
                    .insert(MovesWithOrigin)
                    .insert(ChunkMarker)
                    .id();

                world_map.insert(
                    chunk.position,
                    Chunk::new(
                        entity,
                        chunk.blocks.clone(),
                        chunk
                            .block_state
                            .iter()
                            .map(|(&k, &v)| (k, BlockState(v)))
                            .collect(),
                    ),
                );

                visibility_task_events.send(ComputeVisibleChunkFacesEvent(chunk.position));
            }

            requested.chunks.remove(&chunk.position);
        }
    }
}

// TODO: This doesn't belong in this file
fn handle_block_updates(
    mut commands: Commands,
    origin: Res<Origin>,
    mut world_map: ResMut<WorldMap>,
    net: Res<NetworkClient>,
    mut visibility_task_events: EventWriter<ComputeVisibleChunkFacesEvent>,
    mut block_updates_events: EventReader<NetworkData<messages::BlockUpdates>>,
) {
    for event in block_updates_events.read() {
        let chunk = if let Some(c) = world_map.get_chunk_mut(&event.chunk_position) {
            c
        } else {
            // Server might send block updates that don't belong to the chunks we have loaded?
            continue;
        };

        if chunk.is_uniform() {
            chunk.convert_uniform_to_full();
            let entity = commands
                .spawn(TransformBundle::from(Transform::from_translation(
                    (event.chunk_position - origin.0).as_vec3(),
                )))
                .insert(MovesWithOrigin)
                .insert(ChunkMarker)
                .id();
            chunk.entity = Some(entity);
        }

        let blocks = Blocks::get();
        for (index, block, block_state) in event.blocks.iter() {
            if !blocks.contain(*block) {
                net.disconnect(
                    "Server sent block update with non-existing block, no block \
                    with the id: '{block}'",
                );
                return;
            }

            if chunk[*index] == *block {
                // TODO: On initial chunk reception this gets triggered a bunch of times. The
                // server cannot generate blocks that originate from other chunks, so they are sent
                // as block updates. I assume this is tree leaves, but check that it
                // is not air!
                // TODO: This is perhaps not sound, and it should also make sure the state hasn't
                // changed maybe.
                //
                // Server sends blocks this client has placed, so we skip.
                continue;
            }

            chunk[*index] = *block;

            if let Some(state) = block_state {
                chunk.set_block_state(*index, BlockState(*state));
            } else {
                chunk.remove_block_state(index);
            }
        }

        visibility_task_events.send(ComputeVisibleChunkFacesEvent(event.chunk_position));
    }
}
