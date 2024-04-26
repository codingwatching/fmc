use bevy::{
    prelude::*,
    tasks::{AsyncComputeTaskPool, Task},
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
    chunk_to_subscribers: HashMap<IVec3, HashSet<ConnectionId>>,
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

// TODO: This is too expensive to accommodate many players. I'm thinking chunks can be sorted into
// columns. If it is a chunk that contains blocks, it would be considered a column base. All chunks
// of air in succession above a base would be part of the column. More generally perhaps, all
// chunks that share the same set of visible chunk faces. You will then have a set of arbitrary
// length columns with gaps that you need to traverse. Not obvious to me how to do that, but it
// gives the advantage of traversing the chunks at the world surface(which is the most expensive
// case) column by column, almost converting it from a search in 3d to a search in 2d.
// It might also make sense to split both the column representation and the visible faces part of
// the chunk into its own struct. It is pretty how it is, but I foresee that there will be
// contention for the WorldMap. The locations that borrow it mutably will need all the time they
// can get, and this system will hog it.
//
// TODO: Optimization idea. Instead of using events, use an mpsc. Removes the only need for
// mutability, and so the players can be handled in parallel. Con: Lots of allocation? Keep queues
// for each player. Maybe the search can be done by recursion? How is stack memory even handled
// when it is done in parallel.
//
// Search for chunks by fanning out from the player's chunk position to find chunks that are
// visible to it.
fn subscribe_to_visible_chunks(
    world_map: Res<WorldMap>,
    chunk_subscriptions: Res<ChunkSubscriptions>,
    // NOTE: It's not restricted to running only when the origin is changed. Every time a new chunk
    // is loaded for a player the origin is mutably accessed to trigger the change detection.
    changed_origin_query: Query<
        (&ConnectionId, &PlayerChunkOrigin, &PlayerRenderDistance),
        Changed<PlayerChunkOrigin>,
    >,
    mut subscription_events: EventWriter<SubscribeToChunk>,
    mut queue: Local<Vec<(IVec3, ChunkFace, ChunkFace)>>,
    mut already_visited: Local<HashSet<IVec3>>,
) {
    for (connection_id, chunk_origin, render_distance) in changed_origin_query.iter() {
        already_visited.clear();
        already_visited.insert(chunk_origin.0);

        let subscribed_chunks = chunk_subscriptions
            .subscriber_to_chunks
            .get(connection_id)
            .unwrap();

        if !subscribed_chunks.contains(&chunk_origin.0) {
            subscription_events.send(SubscribeToChunk {
                connection_id: *connection_id,
                chunk_position: chunk_origin.0,
            });
        }

        for chunk_face in [
            ChunkFace::Top,
            ChunkFace::Bottom,
            ChunkFace::Right,
            ChunkFace::Left,
            ChunkFace::Front,
            ChunkFace::Back,
        ] {
            queue.push((
                chunk_face.shift_position(chunk_origin.0),
                chunk_face.opposite(),
                chunk_face.opposite(),
            ));
        }

        // chunk_position = chunk to check
        // from_face = The chunk face the chunk was entered through.
        // main_face = The chunk face entered through at the start of the search.
        while let Some((chunk_position, from_face, main_face)) = queue.pop() {
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

            let surrounding = [
                ChunkFace::Front,
                ChunkFace::Back,
                ChunkFace::Left,
                ChunkFace::Right,
                ChunkFace::Top,
                ChunkFace::Bottom,
            ]
            .into_iter()
            .filter(|face| *face != main_face && *face != from_face);

            for to_face in surrounding {
                let adjacent_position = to_face.shift_position(chunk_position);
                let distance_to_adjacent = (adjacent_position - chunk_origin.0) / CHUNK_SIZE as i32;
                if distance_to_adjacent
                    .abs()
                    .cmpgt(IVec3::splat(render_distance.0 as i32))
                    .any()
                {
                    continue;
                }

                if chunk.is_neighbour_visible(from_face, to_face) {
                    if !already_visited.insert(adjacent_position) {
                        // insert returns false if the position is in the set
                        continue;
                    }

                    queue.push((
                        to_face.shift_position(chunk_position),
                        to_face.opposite(),
                        main_face,
                    ));
                } else if chunk_position == IVec3::new(16, 0, 16) {
                    dbg!("not visible");
                }
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

                        // Apply neighbours' features to the chunk.
                        for terrain_feature in neighbour_chunk.terrain_features.iter() {
                            terrain_feature.apply(&mut chunk, chunk_position);
                        }

                        // Apply chunk's features to the neigbours.
                        for terrain_feature in chunk.terrain_features.iter() {
                            if let Some(changed) = terrain_feature
                                .apply_return_changed(neighbour_chunk, neighbour_position)
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
