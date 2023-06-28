use crate::{
    constants::*,
    game_state::GameState,
    player::Player,
    rendering::chunk::MeshedChunkMarker,
    settings, utils,
    world::{
        blocks::{Block, Blocks},
        world_map::{
            chunk::{Chunk, ChunkMarker, VisibleSides},
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

use super::chunk::VisibleSidesEvent;

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
                    .run_if(in_state(GameState::Playing)),
            )
            .add_systems(
                // This has to be run postupdate because of the async mesh/visible_sides tasks. If
                // it despawns the same tick a task is finished it will panic on trying to insert
                // for non-existing entity. https://github.com/bevyengine/bevy/issues/3845 relevant
                // issue
                PostUpdate,
                chunk_unloading.run_if(resource_changed::<Origin>()),
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
#[derive(Default)]
pub struct ChunkRequestEvent(pub IVec3);

fn pause_system(mut pause: ResMut<Pause>, keyboard_input: Res<Input<KeyCode>>) {
    if keyboard_input.just_pressed(KeyCode::F5) {
        pause.0 = !pause.0;
    }
}

// Removes chunks that are outside the render distance of the player.
fn chunk_unloading(
    net: Res<NetworkClient>,
    origin: Res<Origin>,
    mut world_map: ResMut<WorldMap>,
    settings: Res<settings::Settings>,
    mut commands: Commands,
) {
    // Keep chunks 5 past the render distance to allow for some leeway.
    let max_distance = settings.render_distance as i32 + 5;

    let removed: HashMap<IVec3, Chunk> = world_map
        .chunks
        .extract_if(|chunk_pos, chunk| {
            if (chunk_pos.x - origin.x).abs() / CHUNK_SIZE as i32 > max_distance
                || (chunk_pos.y - origin.y).abs() / CHUNK_SIZE as i32 > max_distance
                || (chunk_pos.z - origin.z).abs() / CHUNK_SIZE as i32 > max_distance
            {
                if let Some(entity) = chunk.entity {
                    commands.entity(entity).despawn_recursive();
                }
                true
            } else {
                false
            }
        })
        .collect();
    net.send_message(messages::UnsubscribeFromChunks {
        chunks: removed.into_keys().collect::<Vec<IVec3>>(),
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
// very messy. AND it would be inherent anti-cheat. AND rendering would bet better at it wouldn't
// have to send partial chunks to the clients. AND this can be removed completely as it could be
// converted to normal frustum culling + speed up as that is parallel.
// TODO: When implementing this I wanted to create chunk columns where all vertically adjacent air
// chunks belonged to the same column (with the first chunk with blocks below them as the column
// base). This would reduce the search drastically when at the surface as you could check entire
// columns in one step, instead of going through all their chunks individually. Didn't do it because
// it was too hard to imagine how it would work. Went with simpler version to save time. Maybe
// implement this or maybe ray tracing can solve it. Meanwhile, it will take up a huge chunk of the
// frame time.
// TODO: The far plane of the frustum is not always vertical, so if you look up and down you can
// see chunks being loaded in and out. Lock the far plane normal vector to {x,0,z}.
// update: Bevy's frustum no longer uses far planes, what is going on?
// TODO: Any fov above some 90 degrees will mess this up. This is because of how it traverses the
// chunk tree. Needs rewrite where it stores the directions it has taken with the chunk, this way
// it does not need to decide a forward vector from the get go, just store [forward,
// first_branch_dir, second_branch_dir] where forward is each direction from the root.
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
            &VisibleSides,
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
    // forward     =      ---------- forward -------------
    //                   /         /         \            \
    // surrounding =   left      right       up          down
    //                 /  \      /  \       /  \         /  \
    // orthogonal  = [up,down] [up,down] [left,right] [left,right]
    let forward_dir = utils::Direction::convert_vector(&view_vector);
    let surrounding = forward_dir.surrounding();
    let orthogonal = forward_dir.orthogonal(&surrounding);

    let mut forward_queue = Vec::with_capacity(settings.render_distance as usize);
    let mut surr_queue = Vec::with_capacity(settings.render_distance as usize);
    let mut ortho_queue = Vec::with_capacity(settings.render_distance.pow(3) as usize);

    let mut already_visited = HashSet::with_capacity(settings.render_distance.pow(3) as usize);

    // Goes one chunk in the direction specified, checks if it's visible and inside the frustum
    // before it adds it to the queue.
    let mut traverse_direction =
        |position: IVec3,
         from_dir: &utils::Direction, // Direction to enter from
         to_dir: &utils::Direction,   // Direction to exit through
         visible_sides: Option<&VisibleSides>,
         queue: &mut Vec<(IVec3, utils::Direction)>| {
            // If visible_sides is None it's an air chunk (all sides are visible)
            // If from_dir is not visible from to_dir, this is where it stops.
            if let Some(visible_sides) = visible_sides {
                if !visible_sides.is_visible(from_dir, to_dir) {
                    return;
                }
            }

            let position = to_dir.shift_chunk_position(position);

            // Only visit a chunk once, no matter which side you enter from
            if already_visited.contains(&position) {
                return;
            }

            // 0.867 is half the diagonal of a 1x1x1 cube, i.e the radius of the sphere around the
            // cube. It is scaled by the side length the chunk.
            if !frustum.intersects_sphere(
                &Sphere {
                    center: (position - origin.0).as_vec3a() + CHUNK_SIZE as f32 / 2.0,
                    radius: 0.867 * CHUNK_SIZE as f32,
                },
                true,
            ) {
                return;
            }

            already_visited.insert(position);
            queue.push((position, to_dir.opposite()));
        };

    // Reads a chunk from the chunk map. If it does not exist, it requests it.
    // Returns Option<Option<VisibleSides>>, first option is if there is a chunk that can be
    // traversed there, the second is what kind of chunk it is.
    // Normal chunk => it has VisibleSides = Some(Some(VisibleSides))
    // uniform and transparent (probably air) => it doesn't have VisibleSides = Some(None)
    // Chunk that hasn't computed its VisibleSides yet => None(None) = None
    macro_rules! read_chunk {
        ($pos:expr) => {{
            if let Some(chunk) = world_map.get_chunk($pos) {
                if let Some(chunk_entity) = chunk.entity {
                    unsafe {
                        if let Ok((entity, visible_sides, visibility, is_meshed_chunk)) =
                            chunk_query.get_unchecked(chunk_entity)
                        {
                            if let Some(mut visibility) = visibility {
                                *visibility = Visibility::Inherited;
                            } else if is_meshed_chunk.is_none() {
                                commands.entity(entity).insert(MeshedChunkMarker);
                            }
                            Some(Some(visible_sides))
                        } else {
                            // visible_sides not finished yet
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
    forward_queue.push((player_chunk_pos, utils::Direction::None));

    while let Some((forward_pos, forward_entry_side)) = forward_queue.pop() {
        let forward_sides = match read_chunk!(&forward_pos) {
            Some(sides) => sides,
            None => continue,
        };

        if forward_entry_side == utils::Direction::None {}

        for (i, surr_dir) in surrounding.iter().enumerate() {
            traverse_direction(
                forward_pos,
                &forward_entry_side,
                surr_dir,
                forward_sides,
                &mut surr_queue,
            );
            while let Some((surr_pos, surr_entry_side)) = surr_queue.pop() {
                let surr_sides = match read_chunk!(&surr_pos) {
                    Some(sides) => sides,
                    None => continue,
                };
                for ortho_dir in orthogonal[i].iter() {
                    traverse_direction(
                        surr_pos,
                        &surr_entry_side,
                        ortho_dir,
                        surr_sides,
                        &mut ortho_queue,
                    );
                    while let Some((ortho_pos, ortho_entry_side)) = ortho_queue.pop() {
                        let ortho_sides = match read_chunk!(&ortho_pos) {
                            Some(sides) => sides,
                            None => continue,
                        };
                        // XXX: This ordering is most likely not correct and might cause visibility
                        // bugs. Complexity too much for me.
                        traverse_direction(
                            ortho_pos,
                            &ortho_entry_side,
                            &forward_dir,
                            ortho_sides,
                            &mut ortho_queue,
                        );
                        traverse_direction(
                            ortho_pos,
                            &ortho_entry_side,
                            surr_dir,
                            ortho_sides,
                            &mut ortho_queue,
                        );
                        traverse_direction(
                            ortho_pos,
                            &ortho_entry_side,
                            ortho_dir,
                            ortho_sides,
                            &mut ortho_queue,
                        );
                    }
                }
                traverse_direction(
                    surr_pos,
                    &surr_entry_side,
                    &forward_dir,
                    surr_sides,
                    &mut surr_queue,
                );
                traverse_direction(
                    surr_pos,
                    &surr_entry_side,
                    surr_dir,
                    surr_sides,
                    &mut surr_queue,
                );
            }
        }

        traverse_direction(
            forward_pos,
            &forward_entry_side,
            &forward_dir,
            forward_sides,
            &mut forward_queue,
        );
    }
}

/// Sends chunk requests to the server.
fn send_chunk_requests(
    mut requested: ResMut<Requested>,
    mut request_events: EventReader<ChunkRequestEvent>,
    net: Res<NetworkClient>,
) {
    let mut chunk_request = messages::ChunkRequest::new();
    for chunk_pos in request_events.iter() {
        if !requested.chunks.contains(&chunk_pos.0) {
            requested.chunks.insert(chunk_pos.0);
            chunk_request.chunks.push(chunk_pos.0);
        }
    }
    if !chunk_request.chunks.is_empty() {
        net.send_message(chunk_request);
    }
}

/// Handles chunks sent from the server.
fn handle_chunk_responses(
    origin: Res<Origin>,
    mut commands: Commands,
    mut world_map: ResMut<WorldMap>,
    mut requested: ResMut<Requested>,
    net: Res<NetworkClient>,
    mut chunk_responses: EventReader<NetworkData<messages::ChunkResponse>>,
    mut visible_sides_events: EventWriter<VisibleSidesEvent>,
    //mut amount: Local<usize>, //chunk_query: Query<(&Chunk, &chunk::Blocks)>,
) {
    for response in chunk_responses.iter() {
        let blocks = Blocks::get();

        for chunk in response.chunks.iter() {
            // TODO: Need to validate block state too. Server can crash client.
            for block_id in chunk.blocks.iter() {
                if !blocks.contains(*block_id) {
                    net.disconnect("Server sent chunk with unknown block id: '{}'");
                    return;
                }
            }

            // TODO: I think there is opportunity here for handling the VisibleSides of uniform
            // chunks. They are either fully transparent or fully opaque. This can be stored so it
            // doesn't have to go through the Blocks when it needs to check. Just an extra
            // bool, and when the chunk is converted to a normal chunk it can be set to false.
            // With this, no uniform chunk would have no entity = speedup for frustum. Now, only
            // chunks that are transparent(air) have no entity.
            if chunk.blocks.len() == 1
                && match &blocks[&chunk.blocks[0]] {
                    Block::Cube(b) if b.quads.len() == 0 => true,
                    _ => false,
                }
            {
                world_map.insert(
                    chunk.position,
                    Chunk::new_air(chunk.blocks.clone(), chunk.block_state.clone()),
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
                    Chunk::new(entity, chunk.blocks.clone(), chunk.block_state.clone()),
                );

                visible_sides_events.send(VisibleSidesEvent(chunk.position));
            }

            requested.chunks.remove(&chunk.position);
        }
    }
}

// TODO: This doesn't feel like it belongs in this file.
// Handles block udates sent from the server.
fn handle_block_updates(
    mut commands: Commands,
    origin: Res<Origin>,
    mut world_map: ResMut<WorldMap>,
    net: Res<NetworkClient>,
    mut visible_sides_events: EventWriter<VisibleSidesEvent>,
    mut block_updates_events: EventReader<NetworkData<messages::BlockUpdates>>,
) {
    for event in block_updates_events.iter() {
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
        for (index, block) in event.blocks.iter() {
            if !blocks.contains(*block) {
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

            if let Some(state) = event.block_state.get(index) {
                chunk.set_block_state(*index, *state);
            } else {
                chunk.remove_block_state(index);
            }
        }

        visible_sides_events.send(VisibleSidesEvent(event.chunk_position));
    }
}
