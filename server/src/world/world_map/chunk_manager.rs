use std::collections::{HashMap, HashSet};

use bevy::{
    prelude::*,
    tasks::{AsyncComputeTaskPool, Task},
};
use fmc_networking::{
    messages::{self, ServerConfig},
    BlockId, ConnectionId, NetworkData, NetworkServer, ServerNetworkEvent,
};
use futures_lite::future;

use crate::{
    bevy_extensions::f64_transform::F64GlobalTransform,
    constants::CHUNK_SIZE,
    database::DatabaseArc,
    players::{PlayerMarker, Players},
    settings::ServerSettings,
    utils,
    world::{
        blocks::Blocks,
        world_map::{
            chunk::{Chunk, ChunkStatus},
            terrain_generation::TerrainGeneratorArc,
            WorldMap,
        },
    },
};

// Handles loading/unloading, generation and sending chunks to the players.
pub struct ChunkManagerPlugin;
impl Plugin for ChunkManagerPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<ChunkUnloadEvent>()
            .add_event::<ChunkSubscriptionEvent>()
            .insert_resource(LoadingTasks::default())
            .insert_resource(WorldMap::default())
            .insert_resource(ChunkSubscriptions::default())
            .add_systems(
                Update,
                (
                    add_chunk_origin,
                    update_chunk_origin,
                    add_render_distance,
                    update_render_distance,
                    handle_subscribers,
                    handle_chunk_requests,
                    handle_chunk_loading_tasks,
                    unsubscribe_from_chunks,
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

fn add_chunk_origin(
    mut commands: Commands,
    player_query: Query<(Entity, &F64GlobalTransform), Added<PlayerMarker>>,
) {
    for (entity, transform) in player_query.iter() {
        let position = transform.translation().as_ivec3();
        commands.entity(entity).insert(PlayerChunkOrigin(position));
    }
}

fn update_chunk_origin(
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
    server_settings: Res<ServerSettings>,
    player_query: Query<Entity, Added<PlayerMarker>>,
) {
    for entity in player_query.iter() {
        commands
            .entity(entity)
            .insert(PlayerRenderDistance(server_settings.render_distance));
    }
}

fn update_render_distance(
    players: Res<Players>,
    server_settings: Res<ServerSettings>,
    mut player_query: Query<&mut PlayerRenderDistance>,
    mut render_distance_events: EventReader<NetworkData<messages::RenderDistance>>,
) {
    for event in render_distance_events.iter() {
        let entity = players.get(&event.source);
        let mut render_distance = player_query.get_mut(entity).unwrap();
        render_distance.0 = event.render_distance.min(server_settings.render_distance);
    }
}

/// Sent when a player subscribes to a new chunk
pub struct ChunkSubscriptionEvent {
    pub connection_id: ConnectionId,
    pub chunk_pos: IVec3,
}

// Event sent when the server should unload a chunk and its associated entities.
pub struct ChunkUnloadEvent(pub IVec3);

// TODO: Add reverse, and remove on player disconnect BEFORE systems have access to it.
// XXX: Attack surface, player can both load chunks too far away, as well as decide not to
// unsubscribe.
// Keeps track of which players are subscribed to what chunks. Clients will get updates for
// everything that happens within a chunk it is subscribed to.
// Chunks are automatically subscribed to when requested.
// The client is responsible for unsubscribing when it no longer wants the updates.
#[derive(Resource, Default)]
pub struct ChunkSubscriptions {
    inner: HashMap<IVec3, HashSet<ConnectionId>>,
    reverse: HashMap<ConnectionId, HashSet<IVec3>>,
}

// TODO: Deriving Default complains about type inference
//impl Default for ChunkSubscriptions {
//    fn default() -> Self {
//        Self {
//            inner: HashMap::new(),
//            reverse: HashMap::new(),
//        }
//    }
//}

impl ChunkSubscriptions {
    pub fn get_subscribers(&self, chunk_pos: &IVec3) -> Option<&HashSet<ConnectionId>> {
        return self.inner.get(chunk_pos);
    }

    // Returns true if the chunk has no subscribers left, false if it does.
    fn unsubscribe(&mut self, chunk_pos: &IVec3, connection: &ConnectionId) -> bool {
        if let Some(subscribers) = self.inner.get_mut(chunk_pos) {
            subscribers.remove(connection);
            self.reverse.get_mut(connection).unwrap().remove(chunk_pos);

            if subscribers.len() == 0 {
                self.inner.remove(chunk_pos);
                return true;
            } else {
                return false;
            }
        } else {
            panic!("Tried to unsubscribe from a chunk that wasn't subscribed to.");
        }
    }

    // Returns true if the chunk is already subscribed to by the connection.
    fn subscribe(&mut self, chunk_pos: IVec3, connection: ConnectionId) -> bool {
        let new = if let Some(chunk_subscribers) = self.inner.get_mut(&chunk_pos) {
            chunk_subscribers.insert(connection)
        } else {
            self.inner.insert(chunk_pos, HashSet::from([connection]));
            true
        };

        if let Some(connection_chunk_subscriptions) = self.reverse.get_mut(&connection) {
            connection_chunk_subscriptions.insert(chunk_pos);
        } else {
            self.reverse.insert(connection, HashSet::from([chunk_pos]));
        }

        return !new;
    }

    fn add_subscriber(&mut self, connection: ConnectionId) {
        self.reverse.insert(connection, HashSet::new());
    }

    fn remove_subscriber(&mut self, connection: &ConnectionId) {
        // TODO: This unrwap can panic, should not be possible. It calls the funciton twice or
        // something.
        for chunk_pos in self.reverse.remove(connection).unwrap().iter() {
            self.inner.get_mut(&chunk_pos).unwrap().remove(connection);
        }
    }
}

fn handle_subscribers(
    mut network_events: EventReader<ServerNetworkEvent>,
    mut chunk_subscriptions: ResMut<ChunkSubscriptions>,
) {
    for event in network_events.iter() {
        match event {
            ServerNetworkEvent::Connected { connection, .. } => {
                chunk_subscriptions.add_subscriber(*connection);
            }
            ServerNetworkEvent::Disconnected(connection) => {
                chunk_subscriptions.remove_subscriber(connection);
            }
            _ => (),
        }
    }
}

#[derive(Component)]
struct ChunkLoadingTask(Task<(IVec3, Chunk)>);

#[derive(Resource, Default, Deref, DerefMut)]
struct LoadingTasks(HashSet<IVec3>);

fn handle_chunk_requests(
    mut commands: Commands,
    net: Res<NetworkServer>,
    database: Res<DatabaseArc>,
    world_map: Res<WorldMap>,
    terrain_generator: Res<TerrainGeneratorArc>,
    mut chunk_subscriptions: ResMut<ChunkSubscriptions>,
    mut loading_tasks: ResMut<LoadingTasks>,
    mut requests: EventReader<NetworkData<messages::ChunkRequest>>,
    mut chunk_subscription_events: EventWriter<ChunkSubscriptionEvent>,
) {
    let thread_pool = AsyncComputeTaskPool::get();

    for request in requests.iter() {
        let mut chunk_response = messages::ChunkResponse::new();

        for chunk_pos in &request.chunks {
            // Clients might send positions that aren't aligned with chunk positions if they are
            // evil, so they need to be normalized.
            let chunk_pos = utils::world_position_to_chunk_position(*chunk_pos);
            for x in -1..=1 {
                for y in -1..=1 {
                    for z in -1..=1 {
                        let chunk_pos = chunk_pos + IVec3::new(x, y, z) * CHUNK_SIZE as i32;

                        if chunk_subscriptions.subscribe(chunk_pos, request.source) {
                            continue;
                        }

                        chunk_subscription_events.send(ChunkSubscriptionEvent {
                            connection_id: request.source,
                            chunk_pos,
                        });

                        if let Some(chunk) = world_map.get_chunk(&chunk_pos) {
                            if chunk.status == ChunkStatus::Finished {
                                chunk_response.add_chunk(
                                    chunk_pos,
                                    chunk.blocks.clone(),
                                    chunk.block_state.clone(),
                                );
                            }
                            continue;
                        }

                        if loading_tasks.contains(&chunk_pos) {
                            continue;
                        } else {
                            loading_tasks.insert(chunk_pos);
                        }

                        let task = thread_pool.spawn(Chunk::load(
                            chunk_pos,
                            terrain_generator.clone(),
                            database.clone(),
                        ));
                        commands.spawn(ChunkLoadingTask(task));
                    }
                }
            }
        }

        if chunk_response.chunks.len() > 0 {
            net.send_one(request.source, chunk_response);
        }
    }
}

// Send generated chunks to clients
fn handle_chunk_loading_tasks(
    mut commands: Commands,
    net: Res<NetworkServer>,
    mut world_map: ResMut<WorldMap>,
    mut loading_tasks: ResMut<LoadingTasks>,
    chunk_subscriptions: Res<ChunkSubscriptions>,
    mut chunks: Query<(Entity, &mut ChunkLoadingTask)>,
) {
    for (entity, mut task) in chunks.iter_mut() {
        if let Some((position, mut chunk)) = future::block_on(future::poll_once(&mut task.0)) {
            loading_tasks.remove(&position);

            for (chunk_offset, partial_chunk) in chunk.partial_chunks.iter() {
                let chunk_pos = position + *chunk_offset;
                let neighbor_chunk = match world_map.get_chunk_mut(&chunk_pos) {
                    Some(c) => c,
                    None => continue,
                };

                let invert_offset = -(*chunk_offset);

                if let Some(neighbors) = chunk.status.neighbors() {
                    neighbors.insert(*chunk_offset, neighbor_chunk.partial_chunks[&invert_offset].clone());
                }

                if let Some(neighbors) = neighbor_chunk.status.neighbors() {
                    neighbors.insert(invert_offset, partial_chunk.clone());
                }

                if neighbor_chunk.try_finish() {
                    // TODO: This fails sometimes, Connections are removed from the pool but they aren't
                    // removed from the chunk subscriptions
                    if let Some(subs) = chunk_subscriptions.get_subscribers(&position) {
                        let mut chunk_response = messages::ChunkResponse::new();

                        chunk_response.add_chunk(
                            chunk_pos,
                            neighbor_chunk.blocks.clone(),
                            neighbor_chunk.block_state.clone(),
                        );
                        net.send_many(subs, chunk_response);
                    }
                }
            }

            if chunk.try_finish() {
                // TODO: This fails sometimes, Connections are removed from the pool but they aren't
                // removed from the chunk subscriptions
                if let Some(subs) = chunk_subscriptions.get_subscribers(&position) {
                    let mut chunk_response = messages::ChunkResponse::new();

                    chunk_response.add_chunk(
                        position,
                        chunk.blocks.clone(),
                        chunk.block_state.clone(),
                    );
                    net.send_many(subs, chunk_response);
                }
            }

            world_map.insert(position, chunk);

            commands.entity(entity).despawn();
        }
    }
}

fn unsubscribe_from_chunks(
    world_map: Res<WorldMap>,
    mut chunk_subscriptions: ResMut<ChunkSubscriptions>,
    mut unload_chunk_events: EventWriter<ChunkUnloadEvent>,
    player_origin_query: Query<
        (&ConnectionId, &PlayerChunkOrigin, &PlayerRenderDistance),
        Changed<PlayerChunkOrigin>,
    >,
) {
    for (connection, origin, render_distance) in player_origin_query.iter() {
        for chunk_pos in chunk_subscriptions.reverse[connection].clone() {
            let distance = (chunk_pos - origin.0).abs() / IVec3::splat(CHUNK_SIZE as i32);
            if distance.cmpgt(IVec3::splat(render_distance.0 as i32)).any() {
                // TODO: This 'contains_chunk' call is a safeguard against unsubscribing from a
                // chunk before it has been sent to the client. It should be temporary until chunk
                // loading is moved fully server side. The edge case here is that a player moves
                // outside the render distance while the chunk is still generating, so the server
                // discards it, but it's left as requested on the client.
                if world_map.contains_chunk(&chunk_pos) && chunk_subscriptions.unsubscribe(&chunk_pos, &connection) {
                    unload_chunk_events.send(ChunkUnloadEvent(chunk_pos));
                }
            }
        }
    }
}

fn unload_chunks(
    mut world_map: ResMut<WorldMap>,
    mut unload_chunk_events: EventReader<ChunkUnloadEvent>,
) {
    for event in unload_chunk_events.iter() {
        world_map.remove_chunk(&event.0);
    }
}
