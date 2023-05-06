use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use bevy::{
    prelude::*,
    tasks::{AsyncComputeTaskPool, Task},
};
use fmc_networking::{messages, ConnectionId, NetworkData, NetworkServer};
use futures_lite::future;

use crate::{
    database::DatabaseArc,
    utils,
    world::{
        blocks::Blocks,
        world_map::{
            chunk::{Chunk, ChunkType},
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
            .insert_resource(WorldMap::default())
            .insert_resource(ChunkSubscriptions::default())
            .add_systems(
                Update,
                (
                    chunk_unloading,
                    handle_chunk_requests,
                    handle_chunk_loading_tasks,
                ),
            );

        //.add_system(update_chunks);
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
#[derive(Resource)]
pub struct ChunkSubscriptions {
    inner: HashMap<IVec3, HashSet<ConnectionId>>,
    reverse: HashMap<ConnectionId, HashSet<IVec3>>,
}

// TODO: Deriving Default complains about type inference
impl Default for ChunkSubscriptions {
    fn default() -> Self {
        Self {
            inner: HashMap::new(),
            reverse: HashMap::new(),
        }
    }
}

impl ChunkSubscriptions {
    pub fn get_subscribers(&self, chunk_pos: &IVec3) -> Option<&HashSet<ConnectionId>> {
        return self.inner.get(chunk_pos);
    }

    /// Returns true if the chunk now has no subscribers, false if it does.
    pub fn unsubscribe(&mut self, chunk_pos: &IVec3, connection: &ConnectionId) -> bool {
        if let Some(subscribers) = self.inner.get_mut(chunk_pos) {
            let was_removed = subscribers.remove(connection);

            if subscribers.len() == 0 {
                self.inner.remove(chunk_pos);
            }

            self.reverse.get_mut(connection).unwrap().remove(chunk_pos);

            return was_removed;
        } else {
            return false;
        }
    }

    pub fn subscribe(&mut self, chunk_pos: IVec3, connection: ConnectionId) {
        if let Some(chunk_subscribers) = self.inner.get_mut(&chunk_pos) {
            chunk_subscribers.insert(connection);
        } else {
            self.inner.insert(chunk_pos, HashSet::from([connection]));
        };

        if let Some(connection_chunk_subscriptions) = self.reverse.get_mut(&connection) {
            connection_chunk_subscriptions.insert(chunk_pos);
        } else {
            self.reverse.insert(connection, HashSet::from([chunk_pos]));
        }
    }

    pub fn remove_subscriber(&mut self, connection: &ConnectionId) {
        // TODO: This unrwap can panic, should not be possible. It calles the funciton twice or
        // something.
        for chunk_pos in self.reverse.remove(connection).unwrap().iter() {
            self.inner.get_mut(&chunk_pos).unwrap().remove(connection);
        }
    }
}

#[derive(Component)]
struct ChunkLoadingTask(Task<(IVec3, Chunk, HashMap<IVec3, Chunk>)>);

// TODO: There's a tiny mismatch between the amount of chunk received by the client and the chunks
// generated. Client measures 7368 chunks while server says there's 7375 when I start it without
// moving the camera.
// TODO: If a user asks for a chunk while it is being generated for someone else
// a new thread will be launched to generate it again. This is because I can't find
// a way to notify handle_chunk_generation_tasks that another user has asked for it
// in the meantime without taking some mutable borrows which I'd rather not.
// I think this is really uncommon though, so might not be a problem at all.
//
/// Sends chunks to the users
fn handle_chunk_requests(
    mut commands: Commands,
    world_map: Res<WorldMap>,
    mut chunk_subscriptions: ResMut<ChunkSubscriptions>,
    net: Res<NetworkServer>,
    terrain_generator: Res<TerrainGeneratorArc>,
    database: Res<DatabaseArc>,
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
            chunk_subscriptions.subscribe(chunk_pos, request.source);
            chunk_subscription_events.send(ChunkSubscriptionEvent {
                connection_id: request.source,
                chunk_pos,
            });

            if let Some(chunk) = world_map.get_chunk(&chunk_pos) {
                match chunk.chunk_type {
                    ChunkType::Normal => {
                        chunk_response.add_chunk(
                            chunk_pos,
                            chunk.blocks.clone(),
                            chunk.block_state.clone(),
                        );
                        continue;
                    }
                    ChunkType::Uniform(block_id) => {
                        chunk_response.add_chunk(chunk_pos, vec![block_id; 1], HashMap::new());
                        continue;
                    }
                    _ => (), // Needs to be generated
                }
            }

            // TODO: Many tasks for the same chunk can be launched. For well behaved clients it
            // (probably) is no problem.
            // Load from disk, or generate if it hasn't been generated.
            let task = thread_pool.spawn(Chunk::load(
                chunk_pos,
                terrain_generator.clone(),
                database.clone(),
            ));
            commands.spawn(ChunkLoadingTask(task));
        }

        if chunk_response.chunks.len() > 0 {
            net.send_one(request.source, chunk_response);
        }
    }
}

// Send generated chunks to clients
fn handle_chunk_loading_tasks(
    mut commands: Commands,
    mut world_map: ResMut<WorldMap>,
    chunk_subscriptions: Res<ChunkSubscriptions>,
    net: Res<NetworkServer>,
    mut chunks: Query<(Entity, &mut ChunkLoadingTask)>,
) {
    for (entity, mut task) in chunks.iter_mut() {
        if let Some((position, chunk, partial_chunks)) =
            future::block_on(future::poll_once(&mut task.0))
        {
            let mut chunk_response = messages::ChunkResponse::new();

            let chunk = match world_map.chunks.entry(position) {
                std::collections::hash_map::Entry::Occupied(mut entry) => {
                    entry.get_mut().combine(chunk);
                    entry.into_mut()
                }
                std::collections::hash_map::Entry::Vacant(entry) => entry.insert(chunk),
            };

            match chunk.chunk_type {
                ChunkType::Normal => {
                    chunk_response.add_chunk(
                        position,
                        chunk.blocks.clone(),
                        chunk.block_state.clone(),
                    );
                }
                ChunkType::Uniform(block_id) => {
                    chunk_response.add_chunk(position, vec![block_id; 1], HashMap::new());
                }
                _ => unreachable!(),
            }

            // TODO: This fails sometimes, Connections are removed from the pool but they aren't
            // removed from the chunk subscriptions
            if let Some(subs) = chunk_subscriptions.get_subscribers(&position) {
                net.send_many(subs, chunk_response);
            }

            for (position, partial_chunk) in partial_chunks.into_iter() {
                match world_map.chunks.entry(position) {
                    std::collections::hash_map::Entry::Occupied(mut entry) => {
                        // Send partial chunks as a set of block updates to clients. Avoids having
                        // to send the entire chunk multiple times.
                        if entry.get_mut().chunk_type != ChunkType::Partial {
                            if let Some(subscribers) =
                                chunk_subscriptions.get_subscribers(&position)
                            {
                                let air = Blocks::get().get_id("air");

                                let blocks = partial_chunk
                                    .blocks
                                    .iter()
                                    .enumerate()
                                    .filter_map(|(index, block)| {
                                        if *block != air {
                                            Some((index, *block))
                                        } else {
                                            None
                                        }
                                    })
                                    .collect();
                                net.send_many(
                                    subscribers,
                                    messages::BlockUpdates {
                                        chunk_position: position,
                                        blocks,
                                        block_state: HashMap::new(),
                                    },
                                )
                            }
                        }

                        entry.get_mut().combine(partial_chunk);
                    }
                    std::collections::hash_map::Entry::Vacant(entry) => {
                        entry.insert(partial_chunk);
                    }
                };
            }

            commands.entity(entity).despawn();
        }
    }
}

// Unload chunks that no player needs anymore.
fn chunk_unloading(
    mut chunk_subscriptions: ResMut<ChunkSubscriptions>,
    mut unsubscribe_events: EventReader<NetworkData<messages::UnsubscribeFromChunks>>,
    mut unload_chunk_events: EventWriter<ChunkUnloadEvent>,
) {
    for event in unsubscribe_events.iter() {
        let connection = event.source;

        for chunk_pos in event.chunks.iter() {
            if chunk_subscriptions.unsubscribe(&chunk_pos, &connection) {
                unload_chunk_events.send(ChunkUnloadEvent(*chunk_pos));
            } else {
                // TODO: This happens sometimes and I don't know why, will not be a problem when
                // chunk loading is removed from client side.
                warn!("{connection}, tried to unsubscribe from a chunk it was not subscribed to");
            }
        }
    }
}
