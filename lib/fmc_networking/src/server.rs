use std::{collections::HashSet, net::SocketAddr, sync::Arc};

use bevy::{prelude::*, utils::Uuid};
use dashmap::DashMap;
use derive_more::Display;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{tcp, TcpListener, TcpStream, ToSocketAddrs},
    runtime::Runtime,
    // TODO: Switch to unbounded so sending is not blocked on the server. It was like this, but
    // there was some unknown memory leak. related perhaps
    // https://github.com/rust-lang/futures-rs/issues/2052
    // still leaks though, just less maybe a bevy issue cause the chunk generator task also blows
    // up a little.
    //sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender},
    sync::mpsc::{channel, Receiver, Sender},
    task::JoinHandle,
};

use crate::{
    error::ServerNetworkError,
    messages::ClientIdentification,
    network_message::{ClientBound, NetworkMessage, ServerBound},
    ConnectionId, NetworkData, NetworkPacket, NetworkSettings, ServerNetworkEvent, SyncChannel,
};

#[derive(Display)]
#[display(fmt = "Incoming Connection from {}", addr)]
struct NewIncomingConnection {
    addr: SocketAddr,
    socket: TcpStream,
    username: String,
}

/// An established connection
pub struct ClientConnection {
    id: ConnectionId,
    receive_task: JoinHandle<()>,
    send_task: JoinHandle<()>,
    send_message: Sender<NetworkPacket>,
    addr: SocketAddr,
}

impl ClientConnection {
    pub fn stop(self) {
        self.receive_task.abort();
        self.send_task.abort();
    }
}

impl std::fmt::Debug for ClientConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClientConnection")
            .field("id", &self.id)
            .field("addr", &self.addr)
            .finish()
    }
}

/// An instance of a [`NetworkServer`] is used to listen for new client connections
/// using [`NetworkServer::listen`]
#[derive(Resource)]
pub struct NetworkServer {
    runtime: Runtime,
    /// Map of network messages that should be sent as bevy events
    recv_message_map: Arc<DashMap<&'static str, Vec<(ConnectionId, Box<dyn NetworkMessage>)>>>,
    /// Map of served connections
    established_connections: Arc<DashMap<ConnectionId, ClientConnection>>,
    /// Connections that have been verified and should be added to the established_connections map.
    new_connections: SyncChannel<NewIncomingConnection>,
    /// Connections that should be disconnected.
    disconnected_connections: SyncChannel<ConnectionId>,
    /// Handle to task that listens for new connections.
    listener_task: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for NetworkServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "NetworkServer [{} Connected Clients]",
            self.established_connections.len()
        )
    }
}

impl NetworkServer {
    pub(crate) fn new() -> NetworkServer {
        NetworkServer {
            runtime: tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("Could not build tokio runtime"),
            recv_message_map: Arc::new(DashMap::new()),
            established_connections: Arc::new(DashMap::new()),
            new_connections: SyncChannel::new(),
            disconnected_connections: SyncChannel::new(),
            listener_task: None,
        }
    }

    /// Start listening for new clients
    ///
    /// ## Note
    /// If you are already listening for new connections, then this will disconnect existing connections first
    pub fn listen(
        &mut self,
        addr: impl ToSocketAddrs + Send + 'static,
    ) -> Result<(), ServerNetworkError> {
        self.stop();

        // Send connection after it's been verified.
        let new_connections = self.new_connections.sender.clone();

        // Listen for new connections at the bind address
        let listen_loop = async move {
            let listener = match TcpListener::bind(addr).await {
                Ok(listener) => listener,
                Err(err) => {
                    error!("Could not bind listen address, Error: {}", err);
                    return;
                }
            };

            loop {
                let (socket, addr) = match listener.accept().await {
                    Ok(v) => v,
                    Err(err) => {
                        error!("Failed to accept connection, Error: {}", err);
                        continue;
                    }
                };
                // TODO: Can choke if someone connects and doesn't identify, it will wait for a
                // timeout before it tries to read the next connection.
                let connection =
                    if let Some((socket, username)) = identify_connection(socket, addr).await {
                        NewIncomingConnection {
                            addr,
                            socket,
                            username,
                        }
                    } else {
                        // The connection failed to verify its identity
                        continue;
                    };

                if let Err(err) = new_connections.send(connection) {
                    error!("Cannot accept new connections, channel closed: {}", err);
                    break;
                }
            }
        };

        trace!("Started listening");

        self.listener_task = Some(self.runtime.spawn(listen_loop));

        return Ok(());
    }

    /// Send a message to one client
    #[track_caller]
    pub fn send_one<T: ClientBound>(&self, connection_id: ConnectionId, message: T) {
        let connection = match self.established_connections.get(&connection_id) {
            Some(conn) => conn,
            None => panic!(
                "Server should not have access to connections that aren't in the connection pool."
            ),
        };

        let packet = NetworkPacket {
            kind: String::from(T::NAME),
            data: Box::new(message),
        };

        if let Err(err) = connection.send_message.blocking_send(packet) {
            error!("There was an error sending a message: {}", err);
            self.disconnect(connection_id);
        }
    }

    /// Send a message to many clients
    #[track_caller]
    pub fn send_many<T: ClientBound + Clone>(
        &self,
        connection_ids: &HashSet<ConnectionId>,
        message: T,
    ) {
        for connection_id in connection_ids.iter() {
            let connection = match self.established_connections.get(connection_id) {
                Some(conn) => conn,
                None => panic!("Server should not have access to connections that aren't in the connection pool."),
            };

            let packet = NetworkPacket {
                kind: String::from(T::NAME),
                data: Box::new(message.clone()),
            };

            match connection.send_message.blocking_send(packet) {
                Ok(_) => (),
                Err(err) => {
                    error!("There was an error sending a message: {}", err);
                    self.disconnect(*connection_id);
                }
            }
        }
    }

    /// Broadcast a message to all connected clients
    pub fn broadcast<T: ClientBound + Clone>(&self, message: T) {
        for connection in self.established_connections.iter() {
            let packet = NetworkPacket {
                kind: String::from(T::NAME),
                data: Box::new(message.clone()),
            };

            match connection.send_message.blocking_send(packet) {
                Ok(_) => (),
                Err(err) => {
                    error!("There was an error sending a message: {}", err);
                    self.disconnect(connection.id);
                }
            }
        }
    }

    /// Disconnect all clients and stop listening for new ones
    ///
    /// ## Notes
    /// This operation is idempotent and will do nothing if you are not actively listening
    pub fn stop(&mut self) {
        if let Some(conn) = self.listener_task.take() {
            conn.abort();
            for conn in self.established_connections.iter() {
                let _ = self.disconnected_connections.sender.send(*conn.key());
            }
            self.established_connections.clear();
            self.recv_message_map.clear();

            self.new_connections.receiver.try_iter().for_each(|_| ());
        }
    }

    /// Disconnect a client
    pub fn disconnect(&self, connection_id: ConnectionId) {
        self.disconnected_connections
            .sender
            .try_send(connection_id)
            .unwrap();
    }
}

// TODO: Make it timeout if it has waited for too long (>0.3s or something).
// TODO: Would be nice if it could verify connections in parallel so it didn't have to block new
// connections.
async fn identify_connection(
    mut socket: TcpStream,
    addr: SocketAddr,
) -> Option<(TcpStream, String)> {
    let length = match socket.read_u32().await {
        Ok(len) => len as usize,
        Err(err) => {
            error!("Encountered error while reading length [{}]: {}", addr, err);
            return None;
        }
    };

    trace!("Received packet with length: {}", length);

    let mut buffer: Vec<u8> = vec![0; length];

    // 1mb, could be set to size of ClientIdentification idk
    if length > 1024 * 1024 {
        error!(
            "Received too large packet from [{}]: {} > {}",
            addr,
            length,
            1024 * 1024
        );
        return None;
    }

    match socket.read_exact(&mut buffer[..length]).await {
        Ok(_) => (),
        Err(err) => {
            error!(
                "Encountered error while reading stream of length {} [{}]: {}",
                length, addr, err
            );
            return None;
        }
    }

    trace!("Read buffer of length {}", length);

    let packet: NetworkPacket = match bincode::deserialize(&buffer[..length]) {
        Ok(packet) => packet,
        Err(err) => {
            error!("Failed to decode network packet from [{}]: {}", addr, err);
            return None;
        }
    };

    let identity: ClientIdentification = match packet.data.downcast() {
        Ok(v) => *v,
        Err(_) => return None,
    };

    return Some((socket, identity.name));
}

async fn recv_task(
    conn_id: ConnectionId,
    recv_message_map: Arc<DashMap<&'static str, Vec<(ConnectionId, Box<dyn NetworkMessage>)>>>,
    network_settings: NetworkSettings,
    mut read_socket: tcp::OwnedReadHalf,
    disconnected_connections: crossbeam_channel::Sender<ConnectionId>,
) {
    let mut buffer: Vec<u8> = vec![0; network_settings.max_packet_length];

    trace!("Starting receive task for {}", conn_id);

    loop {
        trace!("Listening for length!");

        let length = match read_socket.read_u32().await {
            Ok(len) => len as usize,
            Err(err) => {
                // If we get an EOF here, the connection was broken and we simply report a 'disconnected' signal
                if err.kind() == std::io::ErrorKind::UnexpectedEof {
                    break;
                }

                error!(
                    "Encountered error while reading length [{}]: {}",
                    conn_id, err
                );
                break;
            }
        };

        trace!("Received packet with length: {}", length);

        if length > network_settings.max_packet_length {
            error!(
                "Received too large packet from [{}]: {} > {}",
                conn_id, length, network_settings.max_packet_length
            );
            break;
        }

        match read_socket.read_exact(&mut buffer[..length]).await {
            Ok(_) => (),
            Err(err) => {
                error!(
                    "Encountered error while reading stream of length {} [{}]: {}",
                    length, conn_id, err
                );
                break;
            }
        }

        trace!("Read buffer of length {}", length);

        let packet: NetworkPacket = match bincode::deserialize(&buffer[..length]) {
            Ok(packet) => packet,
            Err(err) => {
                error!(
                    "Failed to decode network packet from [{}]: {}",
                    conn_id, err
                );
                break;
            }
        };

        trace!("Created a network packet");

        match recv_message_map.get_mut(&packet.kind[..]) {
            Some(mut packets) => packets.push((conn_id, packet.data)),
            None => {
                error!(
                    "Could not find existing entries for message kind: {:?}",
                    packet
                );
            }
        }

        debug!("Received new message of length: {}", length);
    }

    match disconnected_connections.send(conn_id) {
        Ok(_) => (),
        Err(_) => {
            error!("Could not send disconnected event; channel is disconnected");
        }
    }
}

async fn send_task(
    mut recv_message: Receiver<NetworkPacket>,
    mut send_socket: tcp::OwnedWriteHalf,
    network_settings: NetworkSettings,
) {
    let mut buffer: Vec<u8> = vec![0; network_settings.max_packet_length];

    while let Some(message) = recv_message.recv().await {
        let size = match bincode::serialized_size(&message) {
            Ok(size) => size as usize,
            Err(err) => {
                error!("Could not encode packet {:?}: {}", message, err);
                continue;
            }
        };

        match bincode::serialize_into(&mut buffer[0..size], &message) {
            Ok(_) => (),
            Err(err) => {
                error!(
                    "Could not serialize packet into buffer {:?}: {}",
                    message, err
                );
                continue;
            }
        };

        match send_socket.write_u32(size as u32).await {
            Ok(_) => (),
            Err(err) => {
                error!("Could not send packet length: {:?}: {}", size, err);
                return;
            }
        }

        match send_socket.write_all(&buffer[0..size]).await {
            Ok(_) => (),
            Err(err) => {
                error!("Could not send packet: {:?}: {}", message, err);
                return;
            }
        }
    }
}

pub(crate) fn handle_connections(
    server: Res<NetworkServer>,
    network_settings: Res<NetworkSettings>,
    mut network_events: EventWriter<ServerNetworkEvent>,
) {
    for conn in server.new_connections.receiver.try_iter() {
        match conn.socket.set_nodelay(true) {
            Ok(_) => (),
            Err(e) => error!("Could not set nodelay for [{}]: {}", conn.addr, e),
        }

        let conn_id = ConnectionId {
            uuid: Uuid::new_v4(),
            addr: conn.addr,
        };

        let (read_socket, send_socket) = conn.socket.into_split();

        // XXX: I changed this from an unbounded channel because of some memory issue I could't
        // diagnose.
        let (send_message, recv_message) = channel(10);

        server.established_connections.insert(
            conn_id,
            ClientConnection {
                id: conn_id,
                receive_task: server.runtime.spawn(recv_task(
                    conn_id,
                    server.recv_message_map.clone(),
                    network_settings.clone(),
                    read_socket,
                    server.disconnected_connections.sender.clone(),
                )),
                send_task: server.runtime.spawn(send_task(
                    recv_message,
                    send_socket,
                    network_settings.clone(),
                )),
                send_message,
                addr: conn.addr,
            },
        );

        network_events.send(ServerNetworkEvent::Connected(conn_id, conn.username));
    }
}

// Connections are disconnected with a 1 update-cycle lag. This is let the server application
// process the connection's messages. If the lag wasn't there, the server would recieve the
// disconnect event, while still having messages to process from the connection.
// This way it is guaranteed that there will be no messages when it receives the disconnect event.
pub(crate) fn handle_disconnections(
    server: Res<NetworkServer>,
    mut network_events: EventWriter<ServerNetworkEvent>,
    mut to_disconnect: Local<Vec<ConnectionId>>,
) {
    for conn_id in to_disconnect.drain(..) {
        let connection = match server.established_connections.remove(&conn_id) {
            Some(conn) => conn.1,
            None => continue,
        };

        connection.stop();
        network_events.send(ServerNetworkEvent::Disconnected(conn_id));
    }

    for disconnected_connection in server.disconnected_connections.receiver.try_iter() {
        to_disconnect.push(disconnected_connection);
    }
}

/// A utility trait on [`App`] to easily register [`ServerMessage`]s
pub trait AppNetworkServerMessage {
    /// Register a server message type
    ///
    /// ## Details
    /// This will:
    /// - Add a new event type of [`NetworkData<T>`]
    /// - Register the type for transformation over the wire
    /// - Internal bookkeeping
    fn listen_for_server_message<T: ServerBound>(&mut self) -> &mut Self;
}

impl AppNetworkServerMessage for App {
    fn listen_for_server_message<T: ServerBound>(&mut self) -> &mut Self {
        let server = self.world.get_resource::<NetworkServer>().expect("Could not find `NetworkServer`. Be sure to include the `ServerPlugin` before listening for server messages.");

        debug!("Registered a new ServerMessage: {}", T::NAME);

        assert!(
            !server.recv_message_map.contains_key(T::NAME),
            "Duplicate registration of ServerMessage: {}",
            T::NAME
        );
        server.recv_message_map.insert(T::NAME, Vec::new());
        self.add_event::<NetworkData<T>>();
        self.add_systems(PreUpdate, register_server_message::<T>)
    }
}

fn register_server_message<T>(
    net_res: ResMut<NetworkServer>,
    mut events: EventWriter<NetworkData<T>>,
) where
    T: ServerBound,
{
    let mut messages = match net_res.recv_message_map.get_mut(T::NAME) {
        Some(messages) => messages,
        None => return,
    };

    events.send_batch(
        messages
            .drain(..)
            .flat_map(|(conn, msg)| msg.downcast().map(|msg| NetworkData::new(conn, *msg))),
    );
}
