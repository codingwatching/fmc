use bevy::{
    math::{DQuat, DVec3},
    prelude::*,
};
use serde::{Deserialize, Serialize};

use crate::{
    bevy_extensions::f64_transform::{F64GlobalTransform, F64Transform},
    physics::shapes::Aabb,
    world::items::{ItemStack, ItemStorage, crafting::CraftingTable},
};

// TODO: Mobs would share many of these components, only PlayerMarker is unique really. This could
// benefit from shared systems. The problem is when something changes about a player it needs to be
// sent immediately to reduce latency. Waiting for Changed = a tick's worth of lag. Also I don't
// know where to place the components if they were to be shared.

#[derive(Component, Default)]
pub struct PlayerMarker;

/// Player name shown to other players
#[derive(Component, Deref, DerefMut)]
pub struct PlayerName(pub String);

/// Orientation of the player's camera.
/// The transform's translation is where the camera is relative to the player position.
#[derive(Component, Deref, DerefMut)]
pub struct PlayerCamera(pub F64Transform);

#[derive(Component, Default, Deref, DerefMut, Serialize, Deserialize)]
pub struct PlayerEquipment([ItemStack; 4]);

#[derive(Component, Default, Serialize, Deserialize)]
pub struct PlayerEquippedItem(pub usize);

#[derive(Component, Default, Serialize, Deserialize)]
pub struct PlayerHealth(pub u8);

///// Custom spawn point, not used unless explicitly set
//#[derive(Component)]
//pub struct PlayerSpawnPoint(Vec3);

/// Default bundle used for new players.
#[derive(Bundle)]
pub struct PlayerBundle {
    global_transform: F64GlobalTransform,
    pub transform: F64Transform,
    pub camera: PlayerCamera,
    inventory: ItemStorage,
    equipment: PlayerEquipment,
    equipped_item: PlayerEquippedItem,
    crafting_table: CraftingTable,
    health: PlayerHealth,
    pub aabb: Aabb,
    marker: PlayerMarker,
}

impl Default for PlayerBundle {
    fn default() -> Self {
        Self {
            global_transform: F64GlobalTransform::default(),
            // Put the player somewhere high while it is waiting to be spawned for the first time.
            transform: F64Transform::from_xyz(0.0, 10000.0, 0.0),
            camera: PlayerCamera(F64Transform {
                // XXX: This is hardcoded until a system for changing the player orientation is
                // set up. Also hardcoded in From<PlayerSave>
                translation: DVec3::new(0.3, 1.75, 0.3),
                ..default()
            }),
            inventory: ItemStorage(vec![ItemStack::default(); 36]),
            equipment: PlayerEquipment::default(),
            equipped_item: PlayerEquippedItem::default(),
            crafting_table: CraftingTable(vec![ItemStack::default(); 4]),
            health: PlayerHealth(20),
            aabb: Aabb::from_min_max(DVec3::ZERO, DVec3::new(0.6, 1.8, 0.6)),
            marker: PlayerMarker::default(),
        }
    }
}

/// The format the player is saved as in the database.
#[derive(Serialize, Deserialize)]
pub struct PlayerSave {
    position: DVec3,
    camera_rotation: DQuat,
    inventory: ItemStorage,
    equipment: PlayerEquipment,
    health: PlayerHealth,
}

impl From<PlayerSave> for PlayerBundle {
    fn from(save: PlayerSave) -> Self {
        Self {
            transform: F64Transform::from_translation(save.position),
            camera: PlayerCamera(F64Transform {
                translation: DVec3::new(0.3, 1.8, 0.3),
                rotation: save.camera_rotation,
                ..default()
            }),
            inventory: save.inventory,
            equipment: save.equipment,
            health: save.health,
            // TODO: Remember equipped and send to player
            aabb: Aabb::from_min_max(DVec3::ZERO, DVec3::new(0.6, 1.8, 0.6)),
            ..default()
        }
    }
}
