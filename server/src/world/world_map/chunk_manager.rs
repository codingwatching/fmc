use bevy::{
    prelude::*,
    tasks::{AsyncComputeTaskPool, Task},
    // TODO: This is used instead of std version because extract_if is not stabilized.
    // TODO: I kinda fixed this, but changing it would mean interleaving data access, keep it?
    utils::{HashMap, HashSet},
};
use fmc_networking::{messages, ConnectionId, NetworkData, NetworkServer, ServerNetworkEvent};
use futures_lite::future;

use crate::{
    bevy_extensions::f64_transform::F64GlobalTransform,
    constants::CHUNK_SIZE,
    database::Database,
    players::Player,
    settings::Settings,
    utils,
    world::{
        blocks::BlockState,
        world_map::{
            chunk::{Chunk, ChunkFace},
            terrain_generation::TerrainGenerator,
            WorldMap,
        },
    },
};

// Handles loading/unloading, generation and sending chunks to the players.
pub struct ChunkManagerPlugin;
impl Plugin for ChunkManagerPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<ChunkUnloadEvent>()
            .add_event::<SubscribeToChunk>()
            .insert_resource(WorldMap::default())
            .insert_resource(ChunkSubscriptions::default())
            // This is postupdate so that when a disconnect event is sent, the other systems can
            // assume that the connection is still registered as a subscriber.
            // TODO: This can be changed to run on Update when I sort out the spaghetti in
            // NetworkPlugin.
            .add_systems(PostUpdate, add_and_remove_subscribers)
            .add_systems(
                Update,
                (
                    add_player_chunk_origin,
                    update_player_chunk_origin,
                    add_render_distance,
                    update_render_distance,
                    subscribe_to_visible_chunks,
                    handle_chunk_subscription_events.after(subscribe_to_visible_chunks),
                    unsubscribe_from_chunks,
                    handle_chunk_loading_tasks,
                    unload_chunks,
                ),
            );
    }
}

/// The position of the chunk the player is currently in.
#[derive(Component)]
struct PlayerChunkOrigin(IVec3);

// The max render distance is set by the server. The clients can then send a desired render
// distance that is smaller if they wish, which is stored here.
#[derive(Component)]
struct PlayerRenderDistance(u32);

fn add_player_chunk_origin(
    mut commands: Commands,
    player_query: Query<(Entity, &F64GlobalTransform), Added<Player>>,
) {
    for (entity, transform) in player_query.iter() {
        let position = transform.translation().as_ivec3();
        commands.entity(entity).insert(PlayerChunkOrigin(position));
    }
}

fn update_player_chunk_origin(
    mut player_query: Query<
        (&mut PlayerChunkOrigin, &F64GlobalTransform),
        Changed<F64GlobalTransform>,
    >,
) {
    for (mut chunk_origin, transform) in player_query.iter_mut() {
        let position = transform.translation().as_ivec3();
        let chunk_position = utils::world_position_to_chunk_position(position);
        if chunk_origin.0 != chunk_position {
            chunk_origin.0 = chunk_position;
        }
    }
}

fn add_render_distance(
    mut commands: Commands,
    settings: Res<Settings>,
    player_query: Query<Entity, Added<Player>>,
) {
    for entity in player_query.iter() {
        commands
            .entity(entity)
            .insert(PlayerRenderDistance(settings.render_distance));
    }
}

fn update_render_distance(
    settings: Res<Settings>,
    mut player_query: Query<&mut PlayerRenderDistance>,
    mut render_distance_events: EventReader<NetworkData<messages::RenderDistance>>,
) {
    for event in render_distance_events.read() {
        let mut render_distance = player_query.get_mut(event.source.entity()).unwrap();
        render_distance.0 = event.render_distance.min(settings.render_distance);
    }
}

/// Sent when a player subscribes to a new chunk
#[derive(Event)]
pub struct SubscribeToChunk {
    pub connection_id: ConnectionId,
    pub chunk_position: IVec3,
}

// Event sent when the server should unload a chunk and its associated entities.
#[derive(Event)]
pub struct ChunkUnloadEvent(pub IVec3);

// Keeps track of which players are subscribed to which chunks. Clients will get updates for
// everything that happens within a chunk it is subscribed to.
#[derive(Resource, Default)]
pub struct ChunkSubscriptions {
    // Map from chunk position to all connections that are subscribed to it, mapped by their entity for
    // removal.
    chunk_to_subscribers: HashMap<IVec3, HashSet<ConnectionId>>,
    // reverse
    subscriber_to_chunks: HashMap<ConnectionId, HashSet<IVec3>>,
}

impl ChunkSubscriptions {
    pub fn get_subscribers(
        &self,
        chunk_position: &IVec3,
    ) -> Option<impl IntoIterator<Item = &ConnectionId>> {
        return self.chunk_to_subscribers.get(chunk_position);
    }
}

fn add_and_remove_subscribers(
    mut chunk_subscriptions: ResMut<ChunkSubscriptions>,
    connection_query: Query<&ConnectionId>,
    mut network_events: EventReader<ServerNetworkEvent>,
    mut unload_chunk_events: EventWriter<ChunkUnloadEvent>,
) {
    for event in network_events.read() {
        match event {
            ServerNetworkEvent::Connected { entity, .. } => {
                let connection_id = connection_query.get(*entity).unwrap();
                chunk_subscriptions
                    .subscriber_to_chunks
                    .insert(*connection_id, HashSet::default());
            }
            ServerNetworkEvent::Disconnected { entity } => {
                let connection_id = connection_query.get(*entity).unwrap();
                let subscribed_chunks = chunk_subscriptions
                    .subscriber_to_chunks
                    .remove(connection_id)
                    .unwrap();

                for chunk_position in subscribed_chunks {
                    let subscribers = chunk_subscriptions
                        .chunk_to_subscribers
                        .get_mut(&chunk_position)
                        .unwrap();
                    subscribers.remove(connection_id);

                    if subscribers.len() == 0 {
                        chunk_subscriptions
                            .chunk_to_subscribers
                            .remove(&chunk_position);
                        unload_chunk_events.send(ChunkUnloadEvent(chunk_position));
                    }
                }
            }
            _ => (),
        }
    }
}

fn handle_chunk_subscription_events(
    mut commands: Commands,
    net: Res<NetworkServer>,
    world_map: Res<WorldMap>,
    terrain_generator: Res<TerrainGenerator>,
    database: Res<Database>,
    mut chunk_subscriptions: ResMut<ChunkSubscriptions>,
    mut subscription_events: EventReader<SubscribeToChunk>,
) {
    let thread_pool = AsyncComputeTaskPool::get();

    for event in subscription_events.read() {
        chunk_subscriptions
            .subscriber_to_chunks
            .get_mut(&event.connection_id)
            .unwrap()
            .insert(event.chunk_position);

        if let Some(chunk_subscribers) = chunk_subscriptions
            .chunk_to_subscribers
            .get_mut(&event.chunk_position)
        {
            chunk_subscribers.insert(event.connection_id);
            if let Some(chunk) = world_map.get_chunk(&event.chunk_position) {
                net.send_one(
                    event.connection_id,
                    messages::Chunk {
                        position: event.chunk_position,
                        blocks: chunk.blocks.clone(),
                        block_state: chunk.block_state.clone(),
                    },
                );
            }
        } else {
            chunk_subscriptions
                .chunk_to_subscribers
                .insert(event.chunk_position, HashSet::from([event.connection_id]));

            let task = thread_pool.spawn(Chunk::load(
                event.chunk_position,
                terrain_generator.clone(),
                database.clone(),
            ));
            commands.spawn(ChunkLoadingTask(task));
        };
    }
}

fn unsubscribe_from_chunks(
    chunk_subscriptions: ResMut<ChunkSubscriptions>,
    mut unload_chunk_events: EventWriter<ChunkUnloadEvent>,
    player_origin_query: Query<
        (&ConnectionId, &PlayerChunkOrigin, &PlayerRenderDistance),
        Changed<PlayerChunkOrigin>,
    >,
) {
    // reborrow to make split borrowing work.
    let chunk_subscriptions = chunk_subscriptions.into_inner();
    for (connection_id, origin, render_distance) in player_origin_query.iter() {
        let subscribed_chunks = chunk_subscriptions
            .subscriber_to_chunks
            .get_mut(connection_id)
            .unwrap();
        let removed = subscribed_chunks.extract_if(|chunk_position| {
            let distance = (*chunk_position - origin.0).abs() / CHUNK_SIZE as i32;
            if distance.cmpgt(IVec3::splat(render_distance.0 as i32)).any() {
                return true;
            } else {
                return false;
            }
        });

        for chunk_position in removed {
            let chunk_subscribers = chunk_subscriptions
                .chunk_to_subscribers
                .get_mut(&chunk_position)
                .unwrap();
            chunk_subscribers.remove(connection_id);

            if chunk_subscribers.len() == 0 {
                chunk_subscriptions
                    .chunk_to_subscribers
                    .remove(&chunk_position);
                unload_chunk_events.send(ChunkUnloadEvent(chunk_position));
            }
        }
    }
}

#[derive(Component)]
struct ChunkLoadingTask(Task<(IVec3, Chunk)>);

// Search for chunks by fanning out from the player's chunk position to find chunks that are
// visible to it.
// 1. Fan out from the origin chunk in all directions. The direction the neighbour chunk was
//    entered by is the primary direction, the opposite direction is now blocked.
// 2. If there is a path from the chunk face that was entered through to any of the faces
//    corresponding to the 5 remaining directions add those to the queue. The direction that was
//    entered through this time is the secondary direction unless the primary was used. This step
//    is repeated in the next iteration for the tertiary direction, and locks the path of continued
//    search to those three directions.
// 3. If a chunk has already been checked, it can no longer be added to the queue.
fn subscribe_to_visible_chunks(
    settings: Res<Settings>,
    world_map: Res<WorldMap>,
    chunk_subscriptions: Res<ChunkSubscriptions>,
    changed_origin_query: Query<
        (&ConnectionId, &PlayerChunkOrigin, &PlayerRenderDistance),
        Changed<PlayerChunkOrigin>,
    >,
    mut subscription_events: EventWriter<SubscribeToChunk>,
) {
    let mut already_visited = HashSet::with_capacity(settings.render_distance.pow(3) as usize);
    let mut queue = Vec::new();

    for (connection_id, chunk_origin, render_distance) in changed_origin_query.iter() {
        let subscribed_chunks = chunk_subscriptions
            .subscriber_to_chunks
            .get(connection_id)
            .unwrap();

        queue.push((chunk_origin.0, ChunkFace::None, [ChunkFace::None; 3]));

        // from_face = The chunk face the chunk was entered through.
        // to_faces = The chunk faces it can propagate through
        while let Some((chunk_position, from_face, to_faces)) = queue.pop() {
            let distance_to_chunk = (chunk_position - chunk_origin.0) / CHUNK_SIZE as i32;
            if distance_to_chunk
                .abs()
                .cmpgt(IVec3::splat(render_distance.0 as i32))
                .any()
            {
                // TODO: It would be faster to check this before adding a chunk to the queue.
                continue;
            }

            if !already_visited.insert(chunk_position) {
                // insert returns false if the position is in the set
                continue;
            }

            if !subscribed_chunks.contains(&chunk_position) {
                subscription_events.send(SubscribeToChunk {
                    connection_id: *connection_id,
                    chunk_position,
                });
            }

            let chunk = match world_map.get_chunk(&chunk_position) {
                Some(chunk) => chunk,
                None => {
                    continue;
                }
            };

            if from_face == ChunkFace::None {
                for chunk_face in [
                    ChunkFace::Top,
                    ChunkFace::Bottom,
                    ChunkFace::Right,
                    ChunkFace::Left,
                    ChunkFace::Front,
                    ChunkFace::Back,
                ] {
                    queue.push((
                        chunk_face.shift_position(chunk_position),
                        chunk_face.opposite(),
                        [chunk_face, ChunkFace::None, ChunkFace::None],
                    ));
                }
                continue;
            } else if chunk.is_neighbour_visible(from_face, to_faces[0]) {
                queue.push((
                    to_faces[0].shift_position(chunk_position),
                    to_faces[0].opposite(),
                    to_faces,
                ));
            }

            if to_faces[1] == ChunkFace::None {
                let surrounding = [
                    ChunkFace::Front,
                    ChunkFace::Back,
                    ChunkFace::Left,
                    ChunkFace::Right,
                    ChunkFace::Top,
                    ChunkFace::Bottom,
                ]
                .into_iter()
                .filter(|face| *face != from_face && *face != to_faces[0]);

                for to_face in surrounding {
                    if chunk.is_neighbour_visible(from_face, to_face) {
                        queue.push((
                            to_face.shift_position(chunk_position),
                            to_face.opposite(),
                            [to_faces[0], to_face, ChunkFace::None],
                        ));
                    }
                }

                continue;
            } else if chunk.is_neighbour_visible(from_face, to_faces[1]) {
                queue.push((
                    to_faces[1].shift_position(chunk_position),
                    to_faces[1].opposite(),
                    to_faces,
                ));
            }

            if to_faces[2] == ChunkFace::None {
                let remaining = match to_faces[0] {
                    ChunkFace::Top | ChunkFace::Bottom => match to_faces[1] {
                        ChunkFace::Right | ChunkFace::Left => [ChunkFace::Front, ChunkFace::Back],
                        ChunkFace::Front | ChunkFace::Back => [ChunkFace::Right, ChunkFace::Left],
                        _ => unreachable!(),
                    },
                    ChunkFace::Right | ChunkFace::Left => match to_faces[1] {
                        ChunkFace::Top | ChunkFace::Bottom => [ChunkFace::Front, ChunkFace::Back],
                        ChunkFace::Front | ChunkFace::Back => [ChunkFace::Top, ChunkFace::Bottom],
                        _ => unreachable!(),
                    },
                    ChunkFace::Front | ChunkFace::Back => match to_faces[1] {
                        ChunkFace::Top | ChunkFace::Bottom => [ChunkFace::Right, ChunkFace::Left],
                        ChunkFace::Right | ChunkFace::Left => [ChunkFace::Top, ChunkFace::Bottom],
                        _ => unreachable!(),
                    },
                    ChunkFace::None => unreachable!(),
                };

                for to_face in remaining {
                    if chunk.is_neighbour_visible(from_face, to_face) {
                        queue.push((
                            to_face.shift_position(chunk_position),
                            to_face.opposite(),
                            [to_faces[0], to_faces[1], to_face],
                        ));
                    }
                }
            } else if chunk.is_neighbour_visible(from_face, to_faces[2]) {
                queue.push((
                    to_faces[2].shift_position(chunk_position),
                    to_faces[2].opposite(),
                    to_faces,
                ))
            }
        }
    }
}

fn handle_chunk_loading_tasks(
    mut commands: Commands,
    net: Res<NetworkServer>,
    mut world_map: ResMut<WorldMap>,
    chunk_subscriptions: Res<ChunkSubscriptions>,
    mut origin_query: Query<&mut PlayerChunkOrigin>,
    mut chunks: Query<(Entity, &mut ChunkLoadingTask)>,
) {
    for (entity, mut task) in chunks.iter_mut() {
        if let Some((chunk_position, mut chunk)) = future::block_on(future::poll_once(&mut task.0))
        {
            // TODO: This seems to be a common operation? Maybe create some combination iterator
            // utilily to fight the drift. moore_neigbourhood(n) or something more friendly
            //
            // XXX: If you're wondering where the chunk applies its own terrain features to itself, that
            // happens during chunk generation.
            for x in -1..=1 {
                for y in -1..=1 {
                    for z in -1..=1 {
                        let neighbour_position =
                            chunk_position + IVec3::new(x, y, z) * CHUNK_SIZE as i32;

                        let neighbour_chunk = match world_map.get_chunk_mut(&neighbour_position) {
                            Some(c) => c,
                            // x,y,z = 0, ignored here
                            None => continue,
                        };

                        // Apply neighbour features to the chunk.
                        for terrain_feature in neighbour_chunk.terrain_features.iter() {
                            terrain_feature.apply(&mut chunk, chunk_position);
                        }

                        // Apply chunk's features to the neigbour.
                        for terrain_feature in chunk.terrain_features.iter() {
                            if let Some(changed) =
                                terrain_feature.apply_return_changed(neighbour_chunk, neighbour_position)
                            {
                                if let Some(subscribers) =
                                    chunk_subscriptions.get_subscribers(&neighbour_position)
                                {
                                    net.send_many(
                                        subscribers,
                                        messages::BlockUpdates {
                                            chunk_position: neighbour_position,
                                            blocks: changed,
                                        },
                                    );
                                }
                            }
                        }
                    }
                }
            }

            if let Some(subscribers) = chunk_subscriptions
                .chunk_to_subscribers
                .get(&chunk_position)
            {
                // Triggers 'subscribe_to_visible_chunks' to run again so it can continue from
                // where it last stopped.
                let mut iter = origin_query.iter_many_mut(
                    subscribers
                        .iter()
                        .map(|connection_id| connection_id.entity()),
                );
                while let Some(mut origin) = iter.fetch_next() {
                    origin.set_changed();
                }

                net.send_many(
                    subscribers,
                    messages::Chunk {
                        position: chunk_position,
                        blocks: chunk.blocks.clone(),
                        block_state: chunk.block_state.clone(),
                    },
                );
            }

            world_map.insert(chunk_position, chunk);
            commands.entity(entity).despawn();
        }
    }
}

fn unload_chunks(
    mut world_map: ResMut<WorldMap>,
    mut unload_chunk_events: EventReader<ChunkUnloadEvent>,
) {
    for event in unload_chunk_events.read() {
        world_map.remove_chunk(&event.0);
    }
}
