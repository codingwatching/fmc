use bevy::{math::DVec3, prelude::*};
use fmc_networking_derive::{ClientBound, NetworkMessage, ServerBound};
use serde::{Deserialize, Serialize};

/// Variables that decide how the player should act.
#[derive(NetworkMessage, ClientBound, Serialize, Deserialize, Debug, Clone)]
pub struct PlayerConfiguration {
    /// Camera position relative to the player position.
    pub camera_position: Vec3,
    /// How large the player's AABB should be.
    pub aabb_dimensions: Vec3,
}

// Chesterton: Position and Rotation are separated intentionally. It made for better code
// ergonomics because the camera transform and the player transform are disjoint. Also saves on
// bandwidth(although that might be negligible)
/// A player's position. Used by client to report its position or for the server to dictate.
#[derive(NetworkMessage, ClientBound, ServerBound, Serialize, Deserialize, Debug, Clone)]
pub struct PlayerPosition {
    /// Position of the player.
    pub position: DVec3,
}

/// A player's camera rotation. Used by client to report its facing or for the server to dictate.
#[derive(NetworkMessage, ClientBound, ServerBound, Serialize, Deserialize, Debug, Clone)]
pub struct PlayerCameraRotation {
    /// Where the player camera is looking.
    pub rotation: Quat,
}

/// Send a left click to the server
#[derive(NetworkMessage, ServerBound, Serialize, Deserialize, Debug, Clone)]
pub struct LeftClick;

/// Send a right click to the server.
#[derive(NetworkMessage, ServerBound, Serialize, Deserialize, Debug, Clone)]
pub struct RightClick;
