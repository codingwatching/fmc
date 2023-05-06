use bevy::prelude::Resource;
use fmc_networking_derive::{ClientBound, NetworkMessage, ServerBound};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Configutation of server sent to clients.
#[derive(Resource, NetworkMessage, ClientBound, Serialize, Deserialize, Debug, Clone)]
pub struct ServerConfig {
    /// How far a client should be able to render
    //pub render_distance: u32,
    /// Clients need to determine if they have the correct assets downloaded.
    pub assets_hash: Vec<u8>,
    /// Vec of block filenames ordered by their id.
    pub block_ids: Vec<String>,
    /// Map from model name to id on the server.
    pub model_ids: HashMap<String, u32>,
    /// Map from item name to id on the server.
    pub item_ids: HashMap<String, u32>,
}

/// Clients send this immediately on established connection to identify themselves.
/// If it is not sent, the client will be disconnected.
#[derive(NetworkMessage, ServerBound, Serialize, Deserialize, Debug)]
pub struct ClientIdentification {
    /// The name the player wants to use.
    pub name: String,
}

/// Forceful disconnection by the server.
#[derive(NetworkMessage, ClientBound, Serialize, Deserialize, Debug)]
pub struct Disconnect {
    /// Reason for the disconnect
    pub message: String,
}

// TODO: This is meant to be temporary. As day/night is defined client-side, the server only sends
// the time of day.
/// Sets the time client-side.
#[derive(NetworkMessage, ClientBound, Serialize, Deserialize, Debug, Clone)]
pub struct Time {
    /// Angle of the sun
    pub angle: f32,
}
