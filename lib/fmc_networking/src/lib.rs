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

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use bevy::{prelude::*, utils::Uuid};
use client::AppNetworkClientMessage;
use crossbeam_channel::{unbounded, Receiver, Sender};
use derive_more::{Deref, Display};
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
#[derive(Component, Hash, PartialEq, Eq, Clone, Copy, Display, Debug)]
#[display(fmt = "Connection from {} with ID={}", addr, uuid)]
pub struct ConnectionId {
    uuid: Uuid,
    addr: SocketAddr,
}

impl ConnectionId {
    pub fn default() -> Self {
        Self {
            uuid: Uuid::nil(),
            addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
        }
    }

    pub fn address(&self) -> SocketAddr {
        self.addr
    }

    pub(crate) fn server(addr: Option<SocketAddr>) -> ConnectionId {
        ConnectionId {
            uuid: Uuid::nil(),
            addr: addr.unwrap_or_else(|| SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0)),
        }
    }

    /// Check whether this [`ConnectionId`] is a server
    pub fn is_server(&self) -> bool {
        self.uuid == Uuid::nil()
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
#[derive(Debug)]
pub enum ServerNetworkEvent {
    /// A client has connected
    Connected {
        connection: ConnectionId,
        username: String,
    },
    /// A client has disconnected
    Disconnected(ConnectionId),
    /// An error occured while trying to do a network operation
    Error(ServerNetworkError),
}

#[derive(Debug)]
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
#[derive(Debug, Deref)]
pub struct NetworkData<T> {
    /// The connection information of the sender.
    pub source: ConnectionId,
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
/// Settings to configure the network, both client and server
pub struct NetworkSettings {
    /// Maximum packet size in bytes. If a client ever exceeds this size, they will be disconnected
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
            // Preupdate -> register messages
            // Update -> process messages and register connections that should be disconnected
            // PostUpdate -> Disconnect/Connect clients
            .add_systems(PostUpdate, server::handle_connections)
            .add_systems(PostUpdate, server::handle_disconnections)
            .listen_for_server_message::<messages::ClientFinishedLoading>()
            .listen_for_server_message::<messages::RenderDistance>()
            .listen_for_server_message::<messages::ChunkRequest>()
            .listen_for_server_message::<messages::UnsubscribeFromChunks>()
            .listen_for_server_message::<messages::PlayerCameraRotation>()
            .listen_for_server_message::<messages::PlayerPosition>()
            .listen_for_server_message::<messages::LeftClick>()
            .listen_for_server_message::<messages::RightClick>()
            .listen_for_server_message::<messages::InterfaceTakeItem>()
            .listen_for_server_message::<messages::InterfacePlaceItem>()
            .listen_for_server_message::<messages::InterfaceEquipItem>()
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
            .listen_for_client_message::<messages::InterfaceItemBoxUpdate>()
            .listen_for_client_message::<messages::InterfaceOpen>()
            .listen_for_client_message::<messages::InterfaceClose>()
            .listen_for_client_message::<messages::NewModel>()
            .listen_for_client_message::<messages::DeleteModel>()
            .listen_for_client_message::<messages::ModelUpdateTransform>()
            .listen_for_client_message::<messages::ModelUpdateAsset>()
            .listen_for_client_message::<messages::ChunkResponse>()
            .listen_for_client_message::<messages::ChatMessage>()
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
