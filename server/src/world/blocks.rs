// This module holds everything that has to do with individual blocks.
// To create a new block it has to be defined through a config in the resources. You can find the
// other configs at resources/client/blocks/. Mods can place their configs in their assets/blocks
// directory. The name field of the config will be the name of the block ingame, and has to be
// unique. When a world is generated for the first time, these files will be read, and each block
// name will be associated with an id. This id is constant, and if the config is removed the world
// will not load.
// Each block may have an associated data field of 16 bits, this is sent to the client and used for
// orientation and coloration.
// The code can also define functionality for blocks through the BlockFunctionality trait. This will
// add extra json data as the blocks state. This is useful e.g. for furnaces that need to keep
// track of what is being smelted and what interface should be show to the player when it is
// interacted with.
// TODO: It should store block configs in the worlds database so that worlds are more portable.
//       Addendum: It should store the entire resource folder.
//       It should instead emit warnings when configs(and other things it was initialized with) go
//       missing, and update the database if a config has been changed.
use std::{collections::HashMap, ops::Deref, path::Path};

use bevy::prelude::*;
use fmc_networking::BlockId;
use rand::{distributions::WeightedIndex, prelude::Distribution};
use serde::Deserialize;

use crate::database::{Database, DatabaseArc};

use super::items::ItemId;

mod furnace;
mod liquids;

pub const BLOCK_CONFIG_PATH: &str = "./resources/client/blocks/";

static BLOCKS: once_cell::sync::OnceCell<Blocks> = once_cell::sync::OnceCell::new();

pub struct BlockPlugin;
impl Plugin for BlockPlugin {
    fn build(&self, app: &mut App) {
        // TODO: In the future it needs to be possible for mods to mutate Blocks before it is added
        // to the global. The least painful thing I've come up with is adding Blocks as a temporary
        // resource and then at the end of startup move it. I didn't bother implementing this as
        // bevy 0.1 is soon and that replaces the entire stage system. With how it is now it would
        // be unnecessarily complicated.
        let database = app.world.resource::<DatabaseArc>();
        Blocks::load(database.as_ref());
        //app.add_plugin(liquids::LiquidsPlugin);
        //.add_plugin(furnace::FurnacePlugin);
    }
}

// TODO: Call setup when a block of this type is loaded.
pub trait BlockFunctionality {
    /// Add functionality to a block
    /// The setup function can add custom Components to the entity that the update_system will use.
    fn register_block_functionality(
        &mut self,
        block_name: &str,
        setup: fn(&mut Commands, Entity, Vec<u8>),
    ) -> &mut Self;
}

// premature api
//impl BlockFunctionality for App {
//    fn register_block_functionality(
//        &mut self,
//        block_name: &str,
//        spawn_fn: fn(&mut Commands, Entity, Vec<u8>),
//    ) -> &mut Self {
//        let mut blocks = self.world.get_resource_mut::<Blocks>().expect(
//            "Could not find `Blocks`. Be sure `Blocks::load` is run before `BlockPlugin` is added",
//        );
//        let block = blocks.get_mut_by_name(block_name);
//        block.set_spawn_function(spawn_fn);
//
//        return self;
//    }
//}

pub struct Block {
    config: BlockConfig,
    /// This function is used to set up the ecs entity for the block if it should have
    /// functionality. e.g. a furnace needs ui components and its internal smelting state.
    spawn_entity_fn: Option<fn(&mut Commands, Entity, Vec<u8>)>,
}

impl std::fmt::Debug for Block {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::result::Result<(), std::fmt::Error> {
        f.debug_struct("Block")
            .field("config", &self.config)
            .finish()
    }
}

impl Deref for Block {
    type Target = BlockConfig;

    fn deref(&self) -> &Self::Target {
        &self.config
    }
}

impl Block {
    fn new(config: BlockConfig) -> Self {
        return Self {
            config,
            spawn_entity_fn: None,
        };
    }

    fn set_spawn_function(&mut self, function: fn(&mut Commands, Entity, Vec<u8>)) {
        self.spawn_entity_fn = Some(function);
    }
}

/// The configurations and ids of the blocks in the game.
#[derive(Debug)]
pub struct Blocks {
    // TODO: Can be a vec.
    // block id -> block config
    blocks: HashMap<BlockId, Block>,
    // block name -> block id
    ids: HashMap<String, BlockId>,
}

impl Blocks {
    // Reads blocks from resources/client/blocks/ and resources/server/mods/*/blocks and loads them
    // Each block will have a permanent id assigned to it that will persist through restarts.
    // If a block that has previously been loaded into the world is removed from the assets, the server
    // will fail to load.
    fn load(database: &Database) {
        let mut blocks = Self {
            blocks: HashMap::new(),
            ids: database.load_block_ids(),
        };

        let item_ids = database.load_item_ids();

        for (filename, block_id) in blocks.ids.iter() {
            let file_path = BLOCK_CONFIG_PATH.to_owned() + &filename + ".json";
            let block_config_json = BlockConfigJson::from_file(&file_path);
            let drop = match block_config_json.drop {
                Some(drop) => match BlockDrop::from(&drop, &item_ids) {
                    Ok(d) => Some(d),
                    Err(e) => {
                        panic!("Failed to read 'drop' field for block at: {file_path}\nError: {e}")
                    }
                },
                None => None,
            };
            let block_config = BlockConfig {
                friction: block_config_json.friction,
                hardness: block_config_json.hardness,
                drop,
            };
            blocks.blocks.insert(*block_id, Block::new(block_config));
        }

        BLOCKS.set(blocks).ok();
    }

    pub fn get() -> &'static Self {
        BLOCKS.get().unwrap()
    }

    pub fn get_config(&self, block_id: &BlockId) -> &Block {
        return self.blocks.get(block_id).unwrap();
    }

    pub fn get_mut(&mut self, block_id: &BlockId) -> &mut Block {
        return self.blocks.get_mut(block_id).unwrap();
    }

    pub fn get_mut_by_name(&mut self, block_name: &str) -> &mut Block {
        let id = self.get_id(block_name);
        return self.get_mut(&id);
    }

    #[track_caller]
    pub fn get_id(&self, block_name: &str) -> BlockId {
        if let Some(id) = self.ids.get(block_name) {
            return *id;
        } else {
            // This function is used at startup for the terrain generation, and will fail if the
            // required blocks are not present in the resource pack.
            panic!(
                "Couldn't find id for block with name: '{}'\nMake sure the corresponding block \
                config is present in the resource pack.",
                block_name
            );
        }
    }

    pub fn clone_ids(&self) -> HashMap<String, BlockId> {
        return self.ids.clone();
    }
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum BlockDropJson {
    Single(String),
    Multiple { item: String, count: ItemId },
    Chance(Vec<(f64, Self)>),
}

#[derive(Debug)]
enum BlockDrop {
    Single(ItemId),
    Multiple {
        item: ItemId,
        count: ItemId,
    },
    Chance {
        // The probablities of the drops.
        weights: WeightedIndex<f64>,
        drops: Vec<Self>,
    },
}

impl BlockDrop {
    fn from(json: &BlockDropJson, items: &HashMap<String, ItemId>) -> Result<BlockDrop, String> {
        match json {
            BlockDropJson::Single(item_name) => match items.get(item_name) {
                Some(id) => Ok(Self::Single(*id)),
                None => Err(format!("No item by the name {}", item_name)),
            },
            BlockDropJson::Multiple { item, count } => match items.get(item) {
                Some(id) => Ok(Self::Multiple {
                    item: *id,
                    count: *count,
                }),
                None => Err(format!("No item by the name {}", item)),
            },
            BlockDropJson::Chance(list) => {
                let mut weights = Vec::with_capacity(list.len());
                let mut drops = Vec::with_capacity(list.len());

                for (weight, drop_json) in list {
                    weights.push(*weight);
                    let drop = Self::from(drop_json, items)?;
                    drops.push(drop);
                }

                let weights = match WeightedIndex::new(&weights) {
                    Ok(w) => w,
                    Err(_) => return Err("Weights must be positive and above zero.".to_owned()),
                };

                Ok(Self::Chance { weights, drops })
            }
        }
    }

    fn drop(&self) -> (ItemId, u32) {
        match &self {
            BlockDrop::Single(item) => (*item, 1),
            BlockDrop::Multiple { item, count } => (*item, *count),
            BlockDrop::Chance { weights, drops } => {
                drops[weights.sample(&mut rand::thread_rng())].drop()
            }
        }
    }
}

#[derive(Debug, Deserialize)]
struct BlockConfigJson {
    /// The friction/drag.
    friction: Friction,
    /// How long it takes to break the block without a tool
    hardness: Option<f32>,
    // Which item(s) the block drops
    drop: Option<BlockDropJson>,
}

impl BlockConfigJson {
    fn from_file<P: AsRef<Path>>(path: P) -> Self {
        let file = match std::fs::File::open(&path) {
            Ok(f) => f,
            Err(e) => panic!(
                "Failed to open block config at path: {}\nError: {}",
                path.as_ref().display(),
                e
            ),
        };

        let mut config: serde_json::Value = match serde_json::from_reader(file) {
            Ok(c) => c,
            Err(e) => panic!(
                "Failed to read block config at path: {}\nError: {}",
                path.as_ref().display(),
                e
            ),
        };

        let config = if let Some(parent_file_name) = config["parent"].as_str() {
            let parent_path = std::path::Path::new(BLOCK_CONFIG_PATH)
                .join("parents")
                .join(parent_file_name);
            let mut parent = match Self::read_parent(&parent_path) {
                Ok(p) => p,
                Err(e) => panic!(
                    "Failed to read parent block config for block at path: {}, parent path: {}\nError: {}",
                    path.as_ref().display(),
                    parent_path.display(),
                    e
                ),
            };
            // Overwrite the parent config values with the ones of the child.
            parent
                .as_object_mut()
                .unwrap()
                .append(&mut config.as_object_mut().unwrap());
            serde_json::from_value(parent)
        } else {
            serde_json::from_value(config)
        };

        match config {
            Ok(c) => return c,
            Err(e) => panic!(
                "Failed to read block config at path: {}\nError: {}",
                path.as_ref().display(),
                e
            ),
        }
    }

    fn read_parent(path: &std::path::Path) -> anyhow::Result<serde_json::Value> {
        let file = std::fs::File::open(&path)?;

        let mut config: serde_json::Value = serde_json::from_reader(&file)?;

        // recursively read parent configs
        if let Some(parent) = config["parent"].as_str() {
            let path = std::path::Path::new(BLOCK_CONFIG_PATH).join(parent);
            // TODO: Should probably add some context to the error here to note which files it has
            // read to get here. Easier to debug very nested misconfigured blocks if that will be a
            // thing.
            let mut parent: serde_json::Value = Self::read_parent(&path)?;
            parent
                .as_object_mut()
                .unwrap()
                .append(&mut config.as_object_mut().unwrap());

            return Ok(parent);
        }

        return Ok(config);
    }
}

#[derive(Debug)]
pub struct BlockConfig {
    /// The friction or drag.
    pub friction: Friction,
    /// How long it takes to break the block without a tool, None if unbreakable.
    pub hardness: Option<f32>,
    // Which item(s) the block drops.
    drop: Option<BlockDrop>,
}

impl BlockConfig {
    pub fn drop(&self) -> Option<(ItemId, u32)> {
        if let Some(drop) = &self.drop {
            return Some(drop.drop());
        } else {
            return None;
        }
    }
}

/// The different sides of a block
#[derive(Debug, Hash, PartialEq, Eq, Clone)]
pub enum BlockFace {
    Front,
    Back,
    Right,
    Left,
    Top,
    Bottom,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Friction {
    /// Friction for solid blocks.
    Static {
        front: f32,
        back: f32,
        right: f32,
        left: f32,
        top: f32,
        bottom: f32,
    },
    /// For non-collidable blocks, the friction is instead drag on the player movement.
    Drag(Vec3),
}
