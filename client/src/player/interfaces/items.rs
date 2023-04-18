use std::collections::{HashMap, HashSet};

use bevy::prelude::*;

use fmc_networking::{messages::ServerConfig, BlockId, NetworkClient};
use serde::{Deserialize, Serialize};

use crate::{assets::models::Models, player::hand::ANIMATION_LEN};

pub type ItemId = u32;

pub struct ItemConfig {
    /// Name shown in interfaces
    pub name: String,
    /// Image shown in the interface
    pub image_path: String,
    /// Model id, used to identify item to be equipped
    pub model_id: u32,
    /// The max amount of an item stack of this type
    pub stack_size: u32,
    /// Animation used when holding the item.
    pub equip_animation: Handle<AnimationClip>,
    /// Names used to categorize the item, e.g "helmet". Used to restrict item placement in ui's.
    pub categories: Option<HashSet<String>>,
    /// Block that is placed when the item is used on a surface.
    pub block: Option<BlockId>,
}

#[derive(Deserialize)]
struct Model {
    name: String,
    /// Equipped models are placed in a predetermined position. This here is a position relative to
    /// that position.
    position: Option<Vec3>,
    rotation: Option<f32>,
    scale: Option<f32>,
}

#[derive(Deserialize)]
struct ItemConfigJson {
    name: String,
    image: String,
    equip_model: Model,
    stack_size: u32,
    categories: Option<HashSet<String>>,
    block: Option<String>,
    //properties: serde_json::Map<String, serde_json::Value>,
}

/// Holds the configs of all the items in the game.
#[derive(Resource)]
pub struct Items {
    pub configs: HashMap<ItemId, ItemConfig>,
}

impl Items {
    /// Convenience method for interfaces as items are checked to exist before they are added.
    #[track_caller]
    pub fn get(&self, id: &ItemId) -> &ItemConfig {
        return self.configs.get(id).unwrap();
    }
}

// Ran while loading assets
pub fn load_items(
    mut commands: Commands,
    server_config: Res<ServerConfig>,
    net: Res<NetworkClient>,
    models: Res<Models>,
    mut animations: ResMut<Assets<AnimationClip>>,
) {
    let mut configs = HashMap::new();

    for (filename, id) in server_config.item_ids.iter() {
        let file_path = "server_assets/items/configurations/".to_owned() + filename + ".json";

        let file = match std::fs::File::open(&file_path) {
            Ok(f) => f,
            Err(e) => panic!(
                "Failed to open item config at path: {}\nError: {}",
                &file_path, e
            ),
        };

        let json_config: ItemConfigJson = match serde_json::from_reader(&file) {
            Ok(c) => c,
            Err(e) => {
                net.disconnect(&format!(
                    "Misconfigured resource pack: failed to read item config at: {}.\n\
                        Error: {}",
                    &file_path, e
                ));
                return;
            }
        };

        let model_id = match models.get_id_by_filename(&json_config.equip_model.name) {
            Some(id) => id,
            None => {
                //Server didn't send the correct set of model ids, this should never happen,
                // as the server should read models from the same set of files.
                net.disconnect(&format!(
                    "Misconfigured resource pack: mismatch between model name and ids. \
                        Could not find id for model at path: {}",
                    &file_path
                ));
                return;
            }
        };

        let block_id = match json_config.block {
            Some(name) => match server_config.block_ids.get(&name) {
                Some(block_id) => Some(*block_id),
                None => {
                    net.disconnect(&format!(
                        "Misconfigured resource pack: failed to read item config at: '{}'. \
                            No block with the name '{}'.",
                        &file_path, &name
                    ));
                    return;
                }
            },
            None => None,
        };

        let mut animation = AnimationClip::default();

        let animation_name = Name::new("player_hand");

        let rotation_x = -1.570796;
        let rotation_y = json_config
            .equip_model
            .rotation
            .unwrap_or(std::f32::consts::PI / 2.0);
        let rotation_z = -1.570796;

        // TODO: This looks crude. Needs transform animation and should have a broad curve when
        // swinging and a short reconstitution where it's drawn back in a straight line to the
        // beginning.
        animation.add_curve_to_path(
            EntityPath {
                parts: vec![animation_name.clone()],
            },
            VariableCurve {
                keyframe_timestamps: vec![0.0, ANIMATION_LEN / 2.0, ANIMATION_LEN],
                keyframes: Keyframes::Rotation(vec![
                    Quat::from_rotation_y(rotation_y),
                    Quat::from_rotation_x(rotation_z)
                        * Quat::from_rotation_y(rotation_y)
                        * Quat::from_rotation_x(rotation_x),
                    Quat::from_rotation_y(rotation_y),
                ]),
            },
        );

        let position = json_config.equip_model.position.unwrap_or(Vec3::ZERO);
        animation.add_curve_to_path(
            EntityPath {
                parts: vec![animation_name.clone()],
            },
            VariableCurve {
                keyframe_timestamps: vec![0.0, ANIMATION_LEN / 2.0, ANIMATION_LEN],
                keyframes: Keyframes::Translation(vec![
                    Vec3::new(0.125, -0.1, -0.3) + position,
                    Vec3::new(0.0, -0.2, -0.3),
                    Vec3::new(0.125, -0.1, -0.3) + position,
                ]),
            },
        );

        let scale = Vec3::splat(json_config.equip_model.scale.unwrap_or(0.01));
        animation.add_curve_to_path(
            EntityPath {
                parts: vec![animation_name.clone()],
            },
            VariableCurve {
                keyframe_timestamps: vec![0.0, ANIMATION_LEN],
                keyframes: Keyframes::Scale(vec![scale, scale]),
            },
        );

        let animation_handle = animations.add(animation);

        let config = ItemConfig {
            name: json_config.name,
            image_path: "server_assets/textures/items/".to_owned() + &json_config.image,
            model_id,
            stack_size: json_config.stack_size,
            equip_animation: animation_handle,
            categories: json_config.categories,
            block: block_id,
        };

        configs.insert(*id, config);
    }

    commands.insert_resource(Items { configs });
}

/// ItemStacks are used to represent the data part of an item box in an interface.
#[derive(Debug, Default, Clone, Serialize, Deserialize, Component)]
pub struct ItemStack {
    // The item occupying the stack
    pub item: Option<ItemId>,
    // Maximum amount of the item type that can currently be stored in the stack.
    max_size: Option<u32>,
    // Current stack size.
    pub size: u32,
}

impl ItemStack {
    pub fn new(item: ItemId, max_size: u32, size: u32) -> Self {
        return Self {
            item: Some(item),
            max_size: Some(max_size),
            size,
        };
    }

    fn add(&mut self, amount: u32) {
        self.size += amount;
    }

    pub fn subtract(&mut self, amount: u32) {
        self.size -= amount;
        if self.size == 0 {
            self.item = None;
            self.max_size = None;
        }
    }

    /// Move items into this stack. If this stack already contains items, the stack's items need to
    /// match, otherwise they will be swapped.
    #[track_caller]
    pub fn transfer(&mut self, other: &mut ItemStack, mut amount: u32) -> u32 {
        if other.is_empty() {
            panic!("Tried to transfer from a stack that is empty, this should be asserted by the caller");
        } else if &self.item == &other.item {
            amount = std::cmp::min(amount, self.max_size.unwrap() - self.size);
            self.add(amount);
            other.subtract(amount);
            return amount;
        } else if self.is_empty() {
            self.item = other.item.clone();
            self.max_size = other.max_size.clone();

            amount = std::cmp::min(amount, other.size);

            self.add(amount);
            other.subtract(amount);
            return amount;
        } else {
            self.swap(other);
            return self.size;
        }
    }

    pub fn swap(&mut self, other: &mut ItemStack) {
        std::mem::swap(self, other);
    }

    pub fn is_empty(&self) -> bool {
        return self.item.is_none();
    }
}
