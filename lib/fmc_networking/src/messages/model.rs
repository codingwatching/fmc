use bevy::{math::DVec3, prelude::*};

use fmc_networking_derive::{ClientBound, NetworkMessage};
use serde::{Deserialize, Serialize};

/// Spawn a new model.
#[derive(NetworkMessage, ClientBound, Serialize, Deserialize, Debug, Clone)]
pub struct NewModel {
    /// Id used to reference it when updating. If the same id is sent twice, the old model will be
    /// replaced.
    pub id: u32,
    /// Inherit position/rotation from another model. If the parent transform changes, this model
    /// will change in the same way.
    pub parent_id: Option<u32>,
    /// Position of the model.
    pub position: DVec3,
    /// Rotation of the model.
    pub rotation: Quat,
    /// Scale of the model.
    pub scale: Vec3,
    /// Id of asset that should be used to render the model.
    pub asset: u32,
    /// Index of animation to use when model is standing still.
    pub idle_animation: Option<u32>,
    /// Index of animation to use when model is moving.
    pub moving_animation: Option<u32>,
}

/// Delete an existing model.
#[derive(NetworkMessage, ClientBound, Serialize, Deserialize, Debug, Clone)]
pub struct DeleteModel {
    /// Id used to register the model.
    pub id: u32,
}

/// Update the asset used by a model.
#[derive(NetworkMessage, ClientBound, Serialize, Deserialize, Debug, Clone)]
pub struct ModelUpdateAsset {
    /// Id of the model.
    pub id: u32,
    /// Asset id.
    pub asset: u32,
    /// Index of animation to use when model is standing still.
    pub idle_animation: Option<u32>,
    /// Index of animation to use when model is moving.
    pub moving_animation: Option<u32>,
}

/// Update the transform of a model.
#[derive(NetworkMessage, ClientBound, Serialize, Deserialize, Debug, Clone)]
pub struct ModelUpdateTransform {
    /// Id of the model.
    pub id: u32,
    /// Updated position.
    pub position: DVec3,
    /// Updated rotation.
    pub rotation: Quat,
    /// Updated scale.
    pub scale: Vec3,
}
