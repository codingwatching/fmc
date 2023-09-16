use bevy::prelude::*;
use fmc_networking::BlockId;

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::{
    bevy_extensions::f64_transform::{F64GlobalTransform, F64Transform},
    database::{Database, DatabaseArc},
    utils,
};

pub mod crafting;

//pub use dropped::DropItemEvent;

use super::{
    models::{ModelId, ModelMap},
    world_map::BlockUpdate,
};

pub type ItemId = u32;

pub const ITEM_CONFIG_PATH: &str = "resources/client/items/configurations/";

pub struct ItemPlugin;
impl Plugin for ItemPlugin {
    fn build(&self, app: &mut App) {
        //let database = app.world.resource::<DatabaseArc>();
        //app.insert_resource(Items::load(database.as_ref()));

        app.add_plugins(crafting::CraftingPlugin)
            .add_systems(PreStartup, load_items)
            .add_systems(
                Update,
                (pick_up_items, trigger_physics_update_on_block_change),
            );
    }
}

fn load_items(mut commands: Commands, database: Res<DatabaseArc>) {
    let mut items = Items {
        configs: HashMap::new(),
        ids: database.load_item_ids(),
    };

    for (filename, id) in items.ids.iter() {
        let file_path = ITEM_CONFIG_PATH.to_owned() + filename + ".json";

        let file = match std::fs::File::open(&file_path) {
            Ok(f) => f,
            Err(e) => panic!(
                "Failed to open item config at: {}\nError: {}",
                &file_path, e
            ),
        };

        let json: ItemConfigJson = match serde_json::from_reader(&file) {
            Ok(c) => c,
            Err(e) => panic!(
                "Couldn't read item config from '{}'\nError: {}",
                &file_path, e
            ),
        };

        let blocks = database.load_block_ids();
        let block = match blocks.get(&json.block) {
            Some(block_id) => *block_id,
            None => panic!(
                "Failed to parse item config at: {}\nError: Missing block by the name: {}",
                &file_path, &json.block
            ),
        };

        let models = database.load_model_ids();
        let model_id = match models.get(&json.equip_model.name) {
            Some(id) => *id,
            None => panic!(
                "Failed to parse item config at: {}\nError: Missing model by the name: {}",
                &file_path, &json.equip_model.name
            ),
        };

        items.configs.insert(
            *id,
            ItemConfig {
                name: json.name,
                block,
                model_id,
                max_stack_size: json.stack_size,
                categories: json.categories,
                properties: json.properties,
            },
        );
    }

    commands.insert_resource(items);
}

pub struct ItemConfig {
    /// Name shown in interfaces
    pub name: String,
    /// Block placed by the item
    pub block: BlockId,
    /// Model used to render the item
    pub model_id: ModelId,
    /// The max amount a stack of this item can store
    pub max_stack_size: u32,
    /// Names used to categorize the item, e.g "helmet". Used to restrict item placement in ui's.
    pub categories: Option<HashSet<String>>,
    /// Properties unique to the item
    pub properties: serde_json::Map<String, serde_json::Value>,
}

#[derive(Deserialize)]
pub struct ItemConfigJson {
    pub name: String,
    /// Block name of the block this item can place.
    pub block: String,
    /// Item model filename
    pub equip_model: ModelJson,
    pub stack_size: u32,
    pub categories: Option<HashSet<String>>,
    #[serde(default)]
    pub properties: serde_json::Map<String, serde_json::Value>,
}

#[derive(Deserialize)]
pub struct ModelJson {
    name: String,
}

/// Names and configs of all the items in the game.
#[derive(Resource)]
pub struct Items {
    configs: HashMap<ItemId, ItemConfig>,
    // Map from filename/item name to item id.
    ids: HashMap<String, ItemId>,
}

impl Items {
    #[track_caller]
    pub fn get_config(&self, item_id: &ItemId) -> &ItemConfig {
        return self.configs.get(item_id).unwrap();
    }

    pub fn clone_ids(&self) -> HashMap<String, ItemId> {
        return self.ids.clone();
    }
}

#[derive(Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct Item {
    /// Id assigned to this item type, can be used to lookup properties specific to the item type.
    pub id: ItemId,
    /// Unique properties of the item. Separate from the shared properties of the ItemConfig.
    pub properties: serde_json::Value,
}

impl Item {
    pub fn new(id: ItemId) -> Self {
        return Self {
            id,
            properties: serde_json::Value::default(),
        };
    }
}

// TODO: None of these members should be public, it will cause headache, did for debug
/// An ItemStack holds several of the same item. Used in interfaces.
#[derive(Default, Clone, Serialize, Deserialize)]
pub struct ItemStack {
    /// The item occupying the stack
    pub item: Option<Item>,
    /// Current stack size.
    pub size: u32,
    /// Maximum amount storable in the stack.
    pub max_capacity: u32,
}

impl ItemStack {
    pub fn new(item: Item, size: u32, max_capacity: u32) -> Self {
        return Self {
            item: Some(item),
            size,
            max_capacity,
        };
    }

    pub fn item(&self) -> Option<&Item> {
        return self.item.as_ref();
    }

    pub fn capacity(&self) -> u32 {
        return self.max_capacity - self.size;
    }

    pub fn size(&self) -> u32 {
        return self.size;
    }

    fn add(&mut self, amount: u32) {
        self.size += amount;
    }

    pub fn subtract(&mut self, amount: u32) {
        self.size -= amount;
        if self.size == 0 {
            self.item = None;
            self.max_capacity = 0;
        }
    }

    /// Move items from another stack into this one, if the items are not
    /// the same, swap the stacks.
    #[track_caller]
    pub fn transfer(&mut self, other: &mut ItemStack, mut amount: u32) {
        if other.is_empty() {
            panic!("Tried to transfer from a stack that is empty, this should be asserted by the caller");
        } else if &self.item == &other.item {
            // Transfer as much as is requested, as much as there's room for, or as much as is
            // available.
            amount = std::cmp::min(amount, self.capacity());
            amount = std::cmp::min(amount, other.size());
            self.add(amount);
            other.subtract(amount);
        } else if self.is_empty() {
            self.item = other.item.clone();
            self.max_capacity = other.max_capacity;

            amount = std::cmp::min(amount, other.size());

            self.add(amount);
            other.subtract(amount);
        } else {
            self.swap(other);
        }
    }

    pub fn swap(&mut self, other: &mut ItemStack) {
        std::mem::swap(self, other);
    }

    pub fn is_empty(&self) -> bool {
        return self.item.is_none();
    }
}

/// Generic component used for entities that need to hold items.
#[derive(Component, Deref, DerefMut, Serialize, Deserialize)]
pub struct ItemStorage(pub Vec<ItemStack>);

/// An item that is dropped on the ground.
#[derive(Component, Deref, DerefMut)]
pub struct DroppedItem(pub ItemStack);

fn pick_up_items(
    mut commands: Commands,
    model_map: Res<ModelMap>,
    items: Res<Items>,
    mut players: Query<(&F64GlobalTransform, &mut ItemStorage), Changed<F64GlobalTransform>>,
    mut dropped_items: Query<(Entity, &mut DroppedItem, &F64Transform)>,
) {
    for (player_position, mut player_inventory) in players.iter_mut() {
        let chunk_position =
            utils::world_position_to_chunk_position(player_position.translation().as_ivec3());
        let item_entities = match model_map.get_entities(&chunk_position) {
            Some(e) => e,
            None => continue,
        };

        'outer: for item_entity in item_entities.iter() {
            if let Ok((entity, mut dropped_item, transform)) = dropped_items.get_mut(*item_entity) {
                if transform
                    .translation
                    .distance_squared(player_position.translation())
                    < 2.0
                {
                    let item_config = items.get_config(&dropped_item.item().unwrap().id);

                    for item_stack in player_inventory.iter_mut() {
                        if let Some(item) = item_stack.item() {
                            if item != dropped_item.item().unwrap() || item_stack.capacity() == 0 {
                                continue;
                            }
                            item_stack.transfer(&mut dropped_item.0, u32::MAX);
                        }

                        if dropped_item.is_empty() {
                            commands.entity(entity).despawn();
                            continue 'outer;
                        }
                    }

                    // Iterate twice to first fill up existing stacks before filling empty ones.
                    for item_stack in player_inventory.iter_mut() {
                        if item_stack.is_empty() {
                            *item_stack = ItemStack::new(
                                dropped_item.item().unwrap().clone(),
                                0,
                                item_config.max_stack_size,
                            );
                            item_stack.transfer(&mut dropped_item.0, u32::MAX);
                        }

                        if dropped_item.is_empty() {
                            commands.entity(entity).despawn();
                            continue 'outer;
                        }
                    }
                }
            }
        }
    }
}

fn trigger_physics_update_on_block_change(
    model_map: Res<ModelMap>,
    mut dropped_items: Query<&mut F64Transform, With<DroppedItem>>,
    mut block_updates: EventReader<BlockUpdate>,
) {
    for block_update in block_updates.read() {
        let position = match block_update {
            BlockUpdate::Change { position, .. } => *position,
            _ => continue,
        };
        let chunk_position = utils::world_position_to_chunk_position(position);
        let item_entities = match model_map.get_entities(&chunk_position) {
            Some(e) => e,
            None => continue,
        };

        for entity in item_entities.iter() {
            if let Ok(mut transform) = dropped_items.get_mut(*entity) {
                transform.into_inner();
            }
        }
    }
}

///// Keeps track of items that are on the ground.
//pub struct ItemMap {
//    inner: HashMap<IVec3, Entity>
//}
//
///// Map of item configs loaded from file.
//pub struct ItemConfigs {
//    config: HashMap<u32, ItemConfig>,
//    names: HashMap<String, u32>
//}
//
//impl ItemConfigs {
//    fn get(name: &str) -> &ItemConfig {
//
//    }
//}
