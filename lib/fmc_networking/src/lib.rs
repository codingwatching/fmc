#![deny(
    //missing_docs,
    missing_debug_implementations,
    // why does it need this
    //missing_copy_implementations,
    trivial_casts,
    trivial_numeric_casts,
    unsafe_code,
    unstable_features,
    unused_import_braces,
    unused_qualifications,
    clippy::unwrap_used
)]
#![allow(clippy::type_complexity)]

mod client;
mod error;
mod network_message;
mod server;

pub mod messages;
pub use client::NetworkClient;
pub use server::NetworkServer;

use std::{
    hash::Hash,
    net::{IpAddr, Ipv4Addr, SocketAddr},
};

use bevy::prelude::*;
use client::AppNetworkClientMessage;
use crossbeam_channel::{unbounded, Receiver, Sender};
use derive_more::{Deref, DerefMut, Display};
use error::{ClientNetworkError, ServerNetworkError};
use network_message::NetworkMessage;
use serde::{Deserialize, Serialize};
use server::AppNetworkServerMessage;

// TODO: Should probably define BlockState here too, to avoid hard to parse u16's and easier to
// change data type.
// TODO: I don't remember why I went with an alias instead of newtyping it.
/// Storage type of blocks.
/// Used by both server and client.
pub type BlockId = u16;

struct SyncChannel<T> {
    pub(crate) sender: Sender<T>,
    pub(crate) receiver: Receiver<T>,
}

impl<T> SyncChannel<T> {
    fn new() -> Self {
        let (sender, receiver) = unbounded();

        SyncChannel { sender, receiver }
    }
}

/// A [`ConnectionId`] denotes a single connection
#[derive(Component, PartialEq, Eq, Clone, Copy, Display, Debug)]
#[display(fmt = "Connection from {}", addr)]
pub struct ConnectionId {
    // The entity the connection is attached to doubles as a unique identifier of the connection id. It
    // also comes in handy while handling packets, as you don't need to keep track of the
    // ConnectionId -> Entity relation, it is available through the connection.
    entity: Entity,
    addr: SocketAddr,
}

impl ConnectionId {
    pub fn entity(&self) -> Entity {
        return self.entity;
    }

    pub fn address(&self) -> SocketAddr {
        return self.addr;
    }

    /// Check whether this [`ConnectionId`] is a server
    pub fn is_server(&self) -> bool {
        self.entity == Entity::PLACEHOLDER
    }
}

impl Hash for ConnectionId {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.entity.hash(state);
    }
}

#[derive(Serialize, Deserialize)]
/// [`NetworkPacket`]s are untyped packets to be sent over the wire
struct NetworkPacket {
    kind: String,
    data: Box<dyn NetworkMessage>,
}

impl std::fmt::Debug for NetworkPacket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NetworkPacket")
            .field("kind", &self.kind)
            .finish()
    }
}

/// A network event originating from a [`NetworkServer`]
#[derive(Debug, Event)]
pub enum ServerNetworkEvent {
    /// A client has connected. A ConnectionId has been added to the entity.
    Connected {
        // TODO: Most places I access this ends up mapping the entity to the connection instead of
        // the other way around. Just send the ConnectionId. Same for disconnect.
        entity: Entity,
        username: String,
    },
    /// A client has disconnected. It will be removed at the end of the update cycle.
    Disconnected { entity: Entity },
    /// An error occured while trying to do a network operation
    Error(ServerNetworkError),
}

#[derive(Debug, Event)]
/// A network event originating from a [`NetworkClient`]
pub enum ClientNetworkEvent {
    /// Connected to the server
    Connected,
    /// Disconnected from the server, contains explanation message
    Disconnected(String),
    /// An error occured while trying to do a network operation
    Error(ClientNetworkError),
}

/// [`NetworkData`] are bevy events that should be handled by the receiver.
#[derive(Debug, Deref, DerefMut, Event)]
pub struct NetworkData<T> {
    /// The connection information of the sender.
    pub source: ConnectionId,
    #[deref_mut]
    #[deref]
    inner: T,
}

impl<T> NetworkData<T> {
    pub fn new(source: ConnectionId, inner: T) -> Self {
        Self { source, inner }
    }

    /// Get the inner data out of it
    pub fn into_inner(self) -> T {
        self.inner
    }
}

#[derive(Clone, Debug)]
#[allow(missing_copy_implementations)]
#[derive(Resource)]
/// Settings to configure the network
pub struct NetworkSettings {
    /// Maximum packet size in bytes. If a client ever exceeds this size, it will be disconnected
    /// The default is set to 10MiB
    pub max_packet_length: usize,
}

impl Default for NetworkSettings {
    fn default() -> Self {
        NetworkSettings {
            max_packet_length: 10 * 1024 * 1024,
        }
    }
}

#[derive(Default, Copy, Clone, Debug)]
/// The plugin to add to your bevy app when you want to instantiate a server
pub struct ServerPlugin;

impl Plugin for ServerPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(server::NetworkServer::new())
            .add_event::<ServerNetworkEvent>()
            .init_resource::<NetworkSettings>()
            .add_systems(
                PreUpdate,
                (
                    server::handle_connections,
                    // TODO: I don't know how I feel about this order trickery. I would like it to
                    // be just 'Client Disconnected' -> 'immediately despawn connection entity',
                    // but what do you do with the network messages that arrived in the span
                    // between the last event registration and the disconnection? It would be nice
                    // to handle them, but they are probably not that important. The bigger problem
                    // is that messages have already been added to the message pool, and so are
                    // hard to filter out again. Probably means separate message queues for each
                    // connection, and that is a headache. Maybe a way to this with channels?
                    // HashMap<"packet kind", Sender<NetworkMessage>> passed to recv_task, same but
                    // with Receiver as entity component. Doesn't need to be mutable anywhere so
                    // systems can transfer them to events in parallel.
                    //
                    // It is purposefully 'before' and not 'after' here, so it can go:
                    // 1. Send disconnect event
                    // 2. Application reacts to event, saves player state etc and processes left
                    //    over accumulated network events. 
                    // 3. A tick after, the connection entity is despawned
                    server::handle_disconnection_events.before(server::send_disconnection_events),
                    server::send_disconnection_events
                ),
            )
            .listen_for_server_message::<messages::ClientFinishedLoading>()
            .listen_for_server_message::<messages::RenderDistance>()
            .listen_for_server_message::<messages::PlayerCameraRotation>()
            .listen_for_server_message::<messages::PlayerPosition>()
            .listen_for_server_message::<messages::LeftClick>()
            .listen_for_server_message::<messages::RightClick>()
            .listen_for_server_message::<messages::InterfaceTakeItem>()
            .listen_for_server_message::<messages::InterfacePlaceItem>()
            .listen_for_server_message::<messages::InterfaceEquipItem>()
            .listen_for_server_message::<messages::InterfaceButtonPress>()
            .listen_for_server_message::<messages::InterfaceTextInput>()
            .listen_for_server_message::<messages::AssetRequest>();
    }
}

#[derive(Default, Copy, Clone, Debug)]
/// The plugin to add to your bevy app when you want to instantiate a client
pub struct ClientPlugin;

impl Plugin for ClientPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(client::NetworkClient::new())
            .add_event::<ClientNetworkEvent>()
            .init_resource::<NetworkSettings>()
            .add_systems(PreUpdate, client::handle_connection_event)
            .add_systems(Update, client::handle_client_network_events)
            .listen_for_client_message::<messages::InterfaceTextBoxUpdate>()
            .listen_for_client_message::<messages::InterfaceVisibilityUpdate>()
            .listen_for_client_message::<messages::InterfaceItemBoxUpdate>()
            .listen_for_client_message::<messages::InterfaceOpen>()
            .listen_for_client_message::<messages::InterfaceClose>()
            .listen_for_client_message::<messages::NewModel>()
            .listen_for_client_message::<messages::DeleteModel>()
            .listen_for_client_message::<messages::ModelUpdateTransform>()
            .listen_for_client_message::<messages::ModelUpdateAsset>()
            .listen_for_client_message::<messages::Chunk>()
            .listen_for_client_message::<messages::BlockUpdates>()
            .listen_for_client_message::<messages::ServerConfig>()
            .listen_for_client_message::<messages::AssetResponse>()
            .listen_for_client_message::<messages::Disconnect>()
            .listen_for_client_message::<messages::PlayerConfiguration>()
            .listen_for_client_message::<messages::PlayerCameraRotation>()
            .listen_for_client_message::<messages::PlayerPosition>()
            .listen_for_client_message::<messages::Time>();
    }
}
