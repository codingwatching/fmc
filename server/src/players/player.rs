use bevy::{
    math::{DQuat, DVec3},
    prelude::*,
};

use fmc_networking::messages;
use serde::{Deserialize, Serialize};

use crate::{
    bevy_extensions::f64_transform::{F64GlobalTransform, F64Transform},
    physics::{shapes::Aabb, Velocity},
    world::items::{crafting::CraftingTable, ItemStack, ItemStorage},
};

// TODO: Many of these are not necessarily unique to players. I've marked those I think are with
// the Player prefix. Players probably share most of their properties and functionality with npcs,
// but there are extra concerns about response time, and I don't know what to name the module that
// would keep the components.

#[derive(Component, Default)]
pub struct PlayerMarker;

/// Player name shown to other players
#[derive(Component, Deref, DerefMut)]
pub struct PlayerName(pub String);

/// Orientation of the player's camera.
/// The transform's translation is where the camera is relative to the player position.
#[derive(Component, Deref, DerefMut)]
pub struct Camera(pub F64Transform);

impl Default for Camera {
    fn default() -> Self {
        Self(
            F64Transform {
                translation: DVec3::new(0.3, 1.75, 0.3),
                ..default()
            }
        )
    }
}

/// Helmet, chestplate, leggings, boots in order
#[derive(Component, Default, Deref, DerefMut, Serialize, Deserialize)]
pub struct Equipment([ItemStack; 4]);

#[derive(Component, Default, Serialize, Deserialize)]
pub struct EquippedItem(pub usize);

#[derive(Component, Default, Serialize, Deserialize)]
pub struct Health {
    pub hearts: u32,
    pub max: u32,
}

impl Health {
    pub fn take_damage(&mut self, damage: u32) -> messages::InterfaceVisibilityUpdate {
        let old_hearts = self.hearts;
        self.hearts = self.hearts.saturating_sub(damage);

        let mut image_update = messages::InterfaceVisibilityUpdate::default();
        for i in self.hearts..old_hearts {
            image_update.set_hidden(format!("hotbar/health/{}", i + 1));
        }

        image_update
    }

    pub fn heal(&mut self, healing: u32) -> messages::InterfaceVisibilityUpdate {
        let old_hearts = self.hearts;
        self.hearts = self.hearts.saturating_add(healing).min(self.max);

        let mut image_update = messages::InterfaceVisibilityUpdate::default();
        for i in old_hearts..self.hearts {
            image_update.set_visible(format!("hotbar/health/{}", i + 1));
        }

        image_update
    }
}

#[derive(Component)]
enum GameMode {
    Survival,
    Creative,
}

///// Custom spawn point, not used unless explicitly set
//#[derive(Component)]
//pub struct PlayerSpawnPoint(Vec3);

/// Default bundle used for new players.
#[derive(Bundle)]
pub struct PlayerBundle {
    global_transform: F64GlobalTransform,
    pub transform: F64Transform,
    pub camera: Camera,
    inventory: ItemStorage,
    equipment: Equipment,
    equipped_item: EquippedItem,
    crafting_table: CraftingTable,
    velocity: Velocity,
    health: Health,
    pub aabb: Aabb,
    gamemode: GameMode,
    marker: PlayerMarker,
}

impl Default for PlayerBundle {
    fn default() -> Self {
        Self {
            global_transform: F64GlobalTransform::default(),
            // Put the player somewhere high while it is waiting to be spawned for the first time.
            transform: F64Transform::from_xyz(0.0, 10000.0, 0.0),
            camera: Camera::default(),
            inventory: ItemStorage(vec![ItemStack::default(); 36]),
            equipment: Equipment::default(),
            equipped_item: EquippedItem::default(),
            crafting_table: CraftingTable(vec![ItemStack::default(); 4]),
            velocity: Velocity::default(),
            health: Health {
                hearts: 20,
                max: 20,
            },
            aabb: Aabb::from_min_max(DVec3::ZERO, DVec3::new(0.6, 1.8, 0.6)),
            gamemode: GameMode::Survival,
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
    equipment: Equipment,
    health: Health,
}

impl From<PlayerSave> for PlayerBundle {
    fn from(save: PlayerSave) -> Self {
        Self {
            transform: F64Transform::from_translation(save.position),
            camera: Camera(F64Transform {
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
