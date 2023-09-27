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
//
// TODO: It should store block configs in the worlds database so that worlds are more portable.
//       Addendum: It should store the entire resource folder.
//       It should instead emit warnings when configs(and other things it was initialized with) go
//       missing, and update the database if a config has been changed.
use std::{collections::HashMap, ops::Deref, path::Path};

use bevy::prelude::*;
use fmc_networking::BlockId;
use rand::{distributions::WeightedIndex, prelude::Distribution};
use serde::Deserialize;

use crate::database::Database;

use super::items::ItemId;

mod furnace;
mod water;

pub const BLOCK_CONFIG_PATH: &str = "./resources/client/blocks/";

static BLOCKS: once_cell::sync::OnceCell<Blocks> = once_cell::sync::OnceCell::new();

pub struct BlockPlugin;
impl Plugin for BlockPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreStartup, load_blocks);
        // TODO: In the future it needs to be possible for mods to mutate Blocks before it is added
        // to the global. The least painful thing I've come up with is adding Blocks as a temporary
        // resource and then at the end of startup move it.
        //let database = app.world.resource::<DatabaseArc>();
        //Blocks::load(database.as_ref());
        app.add_plugins(water::WaterPlugin);
        //.add_plugins(furnace::FurnacePlugin);
    }
}

// Reads blocks from resources/client/blocks/ and resources/server/mods/*/blocks and loads them
// Each block will have a permanent id assigned to it that will persist through restarts.
fn load_blocks(database: Res<Database>) {
    fn walk_dir<P: AsRef<std::path::Path>>(dir: P) -> Vec<std::path::PathBuf> {
        let mut files = Vec::new();

        let directory = std::fs::read_dir(dir).expect(
            "Could not read files from block configuration directory, make sure it is present",
        );

        for entry in directory {
            let file_path = entry
                .expect("Failed to read the filenames of the block configs")
                .path();

            if file_path.is_dir() {
                let sub_files = walk_dir(&file_path);
                files.extend(sub_files);
            } else {
                files.push(file_path);
            }
        }

        files
    }

    let mut blocks = Blocks {
        blocks: Vec::new(),
        ids: database.load_block_ids(),
    };

    let item_ids = database.load_item_ids();

    let mut block_ids = blocks.clone_ids();
    let mut maybe_blocks = Vec::new();
    maybe_blocks.resize_with(block_ids.len(), Option::default);

    for file_path in walk_dir(&crate::world::blocks::BLOCK_CONFIG_PATH) {
        let block_config_json = match BlockConfigJson::from_file(&file_path) {
            Some(b) => b,
            None => continue,
        };

        let drop = match block_config_json.drop {
            Some(drop) => match BlockDrop::from(&drop, &item_ids) {
                Ok(d) => Some(d),
                Err(e) => {
                    panic!(
                        "Failed to read 'drop' field for block at: {}\nError: {e}",
                        file_path.display()
                    )
                }
            },
            None => None,
        };

        if let Some(block_id) = block_ids.remove(&block_config_json.name) {
            let block_config = BlockConfig {
                name: block_config_json.name,
                friction: block_config_json.friction,
                hardness: block_config_json.hardness,
                drop,
                is_rotatable: block_config_json.is_rotatable,
            };

            maybe_blocks[block_id as usize] = Some(Block::new(block_config));
        }
    }

    if block_ids.len() > 0 {
        panic!(
            "Misconfigured resource pack, missing blocks: {:?}",
            block_ids.keys().collect::<Vec<_>>()
        );
    }

    blocks.blocks = maybe_blocks.into_iter().flatten().collect();

    BLOCKS.set(blocks).ok();
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
    // block id -> block config
    blocks: Vec<Block>,
    // block name -> block id
    ids: HashMap<String, BlockId>,
}

impl Blocks {
    #[track_caller]
    pub fn get() -> &'static Self {
        BLOCKS.get().unwrap()
    }

    pub fn get_config(&self, block_id: &BlockId) -> &Block {
        return &self.blocks[*block_id as usize];
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

#[derive(Debug, Clone)]
enum BlockDrop {
    Single(ItemId),
    Multiple {
        item: ItemId,
        count: ItemId,
    },
    // TODO: There's no way to define something that drops only one thing n% of the time.
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
    /// Name of the block
    name: String,
    /// The friction/drag.
    friction: Friction,
    /// How long it takes to break the block without a tool
    hardness: Option<f32>,
    // Which item(s) the block drops
    drop: Option<BlockDropJson>,
    #[serde(default)]
    is_rotatable: bool,
}

impl BlockConfigJson {
    fn from_file(path: &Path) -> Option<Self> {
        fn read_as_json_value(
            path: &std::path::Path,
        ) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
            let file = std::fs::File::open(&path)?;

            let mut config: serde_json::Value = serde_json::from_reader(&file)?;

            // recursively read parent configs
            if let Some(parent) = config["parent"].as_str() {
                let parent_path = std::path::Path::new(BLOCK_CONFIG_PATH).join(parent);
                let mut parent: serde_json::Value = match read_as_json_value(&parent_path) {
                    Ok(c) => c,
                    Err(e) => {
                        return Err(format!(
                            "Failed to read parent block config at {}: {}",
                            parent_path.display(),
                            e
                        )
                        .into())
                    }
                };
                parent
                    .as_object_mut()
                    .unwrap()
                    .append(&mut config.as_object_mut().unwrap());

                config = parent;
            }

            Ok(config)
        }

        let json = match read_as_json_value(path) {
            Ok(j) => j,
            Err(e) => panic!("Failed to read block config at {}: {}", path.display(), e),
        };

        if json.get("name").is_some_and(|name| name.is_string()) {
            // TODO: When this fails, theres no way to know which field made it panic.
            return match serde_json::from_value(json) {
                Ok(b) => Some(b),
                Err(e) => panic!("Failed to read block config at {}: {}", path.display(), e),
            };
        } else {
            return None;
        }
    }
}

#[derive(Debug, Clone)]
pub struct BlockConfig {
    /// Name of the block
    pub name: String,
    /// The friction or drag.
    pub friction: Friction,
    /// How long it takes to break the block without a tool, None if unbreakable.
    pub hardness: Option<f32>,
    // Which item(s) the block drops.
    drop: Option<BlockDrop>,
    // If the block is rotatable around the y axis
    pub is_rotatable: bool,
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
#[derive(Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub enum BlockFace {
    Front,
    Back,
    Right,
    Left,
    Top,
    Bottom,
}

impl BlockFace {
    pub fn shift_position(&self, position: IVec3) -> IVec3 {
        match self {
            Self::Front => position + IVec3::Z,
            Self::Back => position - IVec3::Z,
            Self::Right => position + IVec3::X,
            Self::Left => position - IVec3::X,
            Self::Top => position + IVec3::Y,
            Self::Bottom => position - IVec3::Y,
        }
    }

    pub fn to_rotation(&self) -> BlockRotation {
        match self {
            Self::Front => BlockRotation::None,
            Self::Right => BlockRotation::Once,
            Self::Back => BlockRotation::Twice,
            Self::Left => BlockRotation::Thrice,
            _ => unreachable!(),
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
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

#[derive(Default, Hash, PartialEq, Eq, Clone, Copy, Debug)]
pub struct BlockState(pub u16);

impl BlockState {
    pub fn new(rotation: BlockRotation) -> Self {
        return BlockState(rotation as u16);
    }

    pub fn rotation(&self) -> BlockRotation {
        return BlockRotation::from(self.0);
    }

    pub fn set_rotation(&mut self, rotation: BlockRotation) {
        self.0 = self.0 & !0b11 & (rotation as u16 & 0b11);
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(u16)]
pub enum BlockRotation {
    None = 0,
    Once,
    Twice,
    Thrice,
}

impl From<u16> for BlockRotation {
    #[track_caller]
    fn from(value: u16) -> Self {
        return unsafe { std::mem::transmute(value & 0b11) };
    }
}
