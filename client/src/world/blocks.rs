use std::{collections::HashMap, ops::Index, path::PathBuf};

use bevy::prelude::*;
use fmc_networking::{messages, BlockId, NetworkClient};
use serde::Deserialize;

use crate::{
    assets,
    rendering::materials::{self, BlockMaterial},
};

pub static mut BLOCKS: once_cell::sync::OnceCell<Blocks> = once_cell::sync::OnceCell::new();

const MODEL_PATH: &str = "server_assets/textures/models/";

const BLOCK_CONFIG_PATH: &str = "server_assets/blocks/";

const FACE_VERTICES: [[[f32; 3]; 4]; 6] = [
    // Top
    [
        [0.0, 1.0, 0.0],
        [0.0, 1.0, 1.0],
        [1.0, 1.0, 0.0],
        [1.0, 1.0, 1.0],
    ],
    // Front
    [
        [0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [1.0, 0.0, 0.0],
        [1.0, 1.0, 0.0],
    ],
    // Left
    [
        [0.0, 0.0, 1.0],
        [0.0, 1.0, 1.0],
        [0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
    ],
    // Right
    [
        [1.0, 0.0, 0.0],
        [1.0, 1.0, 0.0],
        [1.0, 0.0, 1.0],
        [1.0, 1.0, 1.0],
    ],
    // Back
    [
        [1.0, 0.0, 1.0],
        [1.0, 1.0, 1.0],
        [0.0, 0.0, 1.0],
        [0.0, 1.0, 1.0],
    ],
    // Bottom
    [
        [0.0, 0.0, 1.0],
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 1.0],
        [1.0, 0.0, 0.0],
    ],
];

const FACE_NORMALS: [[f32; 3]; 6] = [
    [0.0, 1.0, 0.0],  // Top
    [0.0, 0.0, -1.0], // Front
    [-1.0, 0.0, 0.0], // Left
    [1.0, 0.0, 0.0],  // Right
    [0.0, 0.0, 1.0],  // Back
    [0.0, -1.0, 0.0], // Bottom
];

const CROSS_VERTICES: [[[f32; 3]; 4]; 2] = [
    [
        [0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [1.0, 0.0, 1.0],
        [1.0, 1.0, 1.0],
    ],
    [
        [0.0, 0.0, 1.0],
        [0.0, 1.0, 1.0],
        [1.0, 0.0, 0.0],
        [1.0, 1.0, 0.0],
    ],
];

const CROSS_NORMALS: [[f32; 3]; 2] = [[1.0, 0.0, -1.0], [-1.0, 0.0, -1.0]];

// TODO: Idk if it makes sense to have this here. Might makes sense to move the load_blocks
// function over to the assets, but keep the Blocks struct here, as it is where you would expect to
// find it.
pub fn load_blocks(
    asset_server: Res<AssetServer>,
    net: Res<NetworkClient>,
    server_config: Res<messages::ServerConfig>,
    block_textures: Res<assets::BlockTextures>,
    material_handles: Res<assets::Materials>,
    materials: Res<Assets<BlockMaterial>>,
) {
    if server_config.block_ids.len() > u16::MAX as usize {
        net.disconnect(&format!(
            "Misconfigured resource pack, too many blocks, {} is the limit, but {} were supplied.",
            BlockId::MAX,
            server_config.block_ids.len()
        ));
        return;
    }

    let mut block_ids = HashMap::with_capacity(server_config.block_ids.len());
    let mut blocks = Vec::with_capacity(server_config.block_ids.len());

    let mut file_path = PathBuf::from(BLOCK_CONFIG_PATH);
    file_path.push("temp");

    for (id, filename) in server_config.block_ids.iter().enumerate() {
        file_path.set_file_name(filename);
        file_path.set_extension("json");

        let block_config = match BlockConfig::from_file(&file_path) {
            Ok(c) => c,
            Err(e) => {
                net.disconnect(&format!(
                    "Misconfigured resource pack, failed to read block config at {}\nError: {}",
                    file_path.display(),
                    e
                ));
                return;
            }
        };

        let block = match block_config {
            BlockConfig::Cube {
                name,
                faces,
                quads,
                friction,
                material,
                only_cull_self,
                interactable,
                light_attenuation,
            } => {
                let material_handle = if let Some(m) = material_handles.get(&material) {
                    m.clone().typed()
                } else {
                    net.disconnect(&format!(
                        "Misconfigured resource pack, tried to use material '{}', but it does not exist.",
                        material
                    ));
                    return;
                };

                let material = materials.get(&material_handle).unwrap();

                let mut mesh_primitives = Vec::new();

                if let Some(faces) = faces {
                    for (i, face_name) in [
                        &faces.top,
                        &faces.front,
                        &faces.left,
                        &faces.right,
                        &faces.back,
                        &faces.bottom,
                    ]
                    .iter()
                    .enumerate()
                    {
                        let texture_array_id = match block_textures.get(face_name) {
                            Some(id) => *id,
                            None => {
                                net.disconnect(format!(
                                    "Misconfigured resource pack, failed to read block at: {}, no block texture with the name {}",
                                    file_path.display(),
                                    face_name
                                ));
                                return;
                            }
                        };

                        let square = QuadPrimitive {
                            vertices: FACE_VERTICES[i],
                            normals: [FACE_NORMALS[i], FACE_NORMALS[i]],
                            texture_array_id,
                            cull_face: Some(match i {
                                    0 => BlockFace::Bottom,
                                    1 => BlockFace::Back,
                                    2 => BlockFace::Right,
                                    3 => BlockFace::Left,
                                    4 => BlockFace::Front,
                                    5 => BlockFace::Top,
                                    _ => unreachable!(),
                                }),
                            light_face: match i {
                                0 => BlockFace::Top,
                                1 => BlockFace::Front,
                                2 => BlockFace::Left,
                                3 => BlockFace::Right,
                                4 => BlockFace::Back,
                                5 => BlockFace::Bottom,
                                _ => unreachable!(),
                            },
                        };

                        mesh_primitives.push(square);
                    }
                }

                if let Some(quads) = quads {
                    for quad in quads.iter() {
                        let texture_array_id = match block_textures.get(&quad.texture) {
                            Some(id) => *id,
                            None => {
                                net.disconnect(format!(
                                    "Misconfigured resource pack, failed to read block at: {}, no block texture with the name {}",
                                    file_path.display(),
                                    &quad.texture
                                ));
                                return;
                            }
                        };

                        let normals = [
                            (Vec3::from_array(quad.vertices[1])
                                - Vec3::from_array(quad.vertices[0]))
                            .cross(
                                Vec3::from_array(quad.vertices[2])
                                    - Vec3::from_array(quad.vertices[1]),
                            )
                            .to_array(),
                            (Vec3::from_array(quad.vertices[3])
                                - Vec3::from_array(quad.vertices[1]))
                            .cross(
                                Vec3::from_array(quad.vertices[2])
                                    - Vec3::from_array(quad.vertices[1]),
                            )
                            .to_array(),
                        ];

                        let normal = Vec3::from(normals[0]);
                        let normal_max =
                            normal.abs().cmpeq(Vec3::splat(normal.abs().max_element()));
                        let light_face = if normal_max.x {
                            if normal.x.is_sign_positive() {
                                BlockFace::Right
                            } else {
                                BlockFace::Left
                            }
                        } else if normal_max.y {
                            if normal.y.is_sign_positive() {
                                BlockFace::Top
                            } else {
                                BlockFace::Bottom
                            }
                        } else if normal_max.z {
                            if normal.z.is_sign_positive() {
                                BlockFace::Front
                            } else {
                                BlockFace::Back
                            }
                        } else {
                            unreachable!();
                        };

                        mesh_primitives.push(QuadPrimitive {
                            vertices: quad.vertices,
                            normals,
                            texture_array_id,
                            cull_face: quad.cull_face,
                            light_face,
                        });
                    }
                }

                let cull_method = if only_cull_self {
                    CullMethod::OnlySelf
                } else {
                    match material.alpha_mode {
                        AlphaMode::Opaque => CullMethod::All,
                        AlphaMode::Mask(_) => CullMethod::None,
                        _ => CullMethod::TransparentOnly
                    }
                };

                Block::Cube(Cube {
                    name,
                    material_handle,
                    quads: mesh_primitives,
                    friction,
                    interactable,
                    cull_method,
                    light_attenuation: light_attenuation.unwrap_or(15).min(15),
                })
            }

            BlockConfig::Model {
                name,
                center_model,
                side_model,
                friction,
                cull_faces,
                interactable,
            } => {
                let center_model = if let Some(center_model) = center_model {
                    let path = MODEL_PATH.to_owned() + &center_model.name + ".glb#Scene0";
                    Some((
                        // It struggles with inferring type?
                        asset_server.load::<Scene, &String>(&path).cast_weak(),
                        Transform {
                            translation: center_model.position,
                            rotation: center_model.rotation,
                            scale: Vec3::ONE,
                        },
                    ))
                } else {
                    None
                };

                let side_model = if let Some(side_model) = side_model {
                    let path = MODEL_PATH.to_owned() + &side_model.name + ".glb#Scene0";
                    Some((
                        asset_server.load::<Scene, &String>(&path).cast_weak(),
                        Transform {
                            translation: side_model.position,
                            rotation: side_model.rotation,
                            scale: Vec3::ONE,
                        },
                    ))
                } else {
                    None
                };

                if center_model.is_none() && side_model.is_none() {
                    net.disconnect(format!(
                        "Misconfigured resource pack, failed to read block at: {}, \
                        one of 'center_model' and 'side_model' must be defined",
                        file_path.display()
                    ));
                    return;
                }

                Block::Model(BlockModel {
                    name,
                    center: center_model,
                    side: side_model,
                    friction,
                    cull_faces,
                    interactable,
                })
            }
        };

        block_ids.insert(block.name().to_owned(), id as BlockId);
        blocks.push(block);
    }

    unsafe {
        BLOCKS.take();
        BLOCKS.set(Blocks { blocks, block_ids }).unwrap();
    }
}

// TODO: Wrap into Blocks(_Blocks)? This way it can have 2 get functions. One for the OnceCell and
// one for getting blocks. Just implement deref for blocks. [index] for blocks looks really
// awkward.
/// The configurations for all the blocks.
#[derive(Debug, Default)]
pub struct Blocks {
    blocks: Vec<Block>,
    // Map from block name to block id
    block_ids: HashMap<String, BlockId>,
}

impl Blocks {
    #[track_caller]
    pub fn get() -> &'static Self {
        unsafe {
            return BLOCKS
                .get()
                .expect("The blocks have not been loaded yet, make sure this is only used after.");
        }
    }

    /// Use when reading blocks directly from server connection. It is unknown if it is a valid
    /// block.
    pub fn get_config(&self, block_id: &BlockId) -> Option<&Block> {
        return self.blocks.get(*block_id as usize);
    }

    pub fn get_id(&self, name: &str) -> Option<&BlockId> {
        return self.block_ids.get(name);
    }

    pub fn contains(&self, block_id: BlockId) -> bool {
        return block_id as usize <= self.blocks.len();
    }
}

impl Index<&BlockId> for Blocks {
    type Output = Block;

    fn index(&self, index: &BlockId) -> &Self::Output {
        return &self.blocks[*index as usize];
    }
}

#[derive(Debug)]
pub struct Cube {
    /// Name of the block
    pub name: String,
    /// Material used to render this block.
    pub material_handle: Handle<materials::BlockMaterial>,
    /// List of squares meshes that make up the block.
    pub quads: Vec<QuadPrimitive>,
    /// Friction value for player contact.
    pub friction: Friction,
    /// If when the player uses their equipped item on this block it should count as an
    /// interaction, or it should count as trying to place its associated block.
    pub interactable: bool,
    /// The alpha mode of the blocks associated material, used to determine face culling.
    cull_method: CullMethod,
    /// If transparent, should this decrease the vertical sunlight level.
    pub light_attenuation: u8,
}

// TODO: This was made before the Models collection was made. This could hold model ids instead of
// the handles. I have hardcoded the glb extension here, which would no longer be a thing.
//
// Models are used to render all blocks of irregular shape. There are multiple ways to place
// the model inside the cube. The server sends an orientation for all block models part of a
// chunk which define if it should be placed on the side of the block or in the center, if it
// should be upside down, and which direction it should point. Meanwhile when the player places
// a block, if the bottom surface is clicked it will place the center model(if defined) in the
// direction facing the player. If a side is clicked it will try to place the side model, if
// that is not available, it will fall back to the center model. One of them is always defined.
#[derive(Debug)]
pub struct BlockModel {
    /// Name of the block
    pub name: String,
    /// Model used when centered in the block, weak handle
    pub center: Option<(Handle<Scene>, Transform)>,
    /// Model used when on the side of the block, weak handle
    pub side: Option<(Handle<Scene>, Transform)>,
    /// Friction or drag, applied by closest normal of the textures.
    pub friction: Friction,
    /// Which of the blocks faces obstruct the view of adjacent blocks.
    pub cull_faces: ModelCullFaces,
    /// If when the player uses their equipped item on this block, it should count as an
    /// interaction, or it should count as trying to place a block.
    pub interactable: bool,
}

#[derive(Debug)]
pub enum Block {
    Cube(Cube),
    Model(BlockModel),
}

impl Block {
    pub fn culls(&self, other: &Block, face: BlockFace) -> bool {
        match self {
            Block::Cube(cube) => {
                let Block::Cube(other_cube) = other else { unreachable!() };
                match cube.cull_method {
                    CullMethod::All => true,
                    CullMethod::None => false,
                    CullMethod::TransparentOnly => other_cube.cull_method == CullMethod::TransparentOnly,
                    // TODO: This isn't correct on purpose, the blocks should be compared. Could be by id,
                    // but I don't have that here. Comparing by name is expensive, don't want to.
                    // Will fuck up if two different blocks are put together. Can use const* to
                    // compare pointer?
                    CullMethod::OnlySelf => other_cube.cull_method == CullMethod::OnlySelf
                }
            }
            Block::Model(model) => {
                match face {
                    BlockFace::Front => model.cull_faces.front,
                    BlockFace::Back => model.cull_faces.back,
                    BlockFace::Top => model.cull_faces.top,
                    BlockFace::Bottom => model.cull_faces.bottom,
                    BlockFace::Left => model.cull_faces.left,
                    BlockFace::Right => model.cull_faces.right,
                }
            }
        }
    }

    pub fn is_transparent(&self) -> bool {
        match self {
            Block::Cube(c) => match c.cull_method {
                CullMethod::All => false,
                _ => true
            },
            Block::Model(_) => true,
        }
    }

    pub fn friction(&self) -> &Friction {
        match self {
            Block::Cube(cube) => &cube.friction,
            Block::Model(model) => &model.friction,
        }
    }

    pub fn light_attenuation(&self) -> u8 {
        match self {
            Block::Cube(c) => c.light_attenuation,
            Block::Model(_) => 1,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Block::Cube(c) => &c.name,
            Block::Model(m) => &m.name,
        }
    }
}

/// Block config that is stored on file.
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum BlockConfig {
    // There is easy way to define a cube, and hard. Give 'faces' and it will generate cube mesh on
    // its own. Give quads and the cube can take on non-cube shapes.
    Cube {
        /// Name of the block, must be unique
        name: String,
        /// Convenient way to define a block as opposed to having to define it through the quads.
        faces: Option<TextureNames>,
        /// List of quads that make up a mesh.
        quads: Option<Vec<QuadPrimitiveJson>>,
        /// The friction or drag.
        friction: Friction,
        /// Material that should be used to render
        material: String,
        /// If the block should only cull quads from blocks of the same type.
        #[serde(default)]
        only_cull_self: bool,
        /// If the block is interactable
        #[serde(default)]
        interactable: bool,
        /// How many levels light should decrease when passing through this block.
        light_attenuation: Option<u8>,
    },
    Model {
        /// Name of the block, must be unique
        name: String,
        /// Name of model used when placed in the center of the block
        center_model: Option<ModelConfig>,
        /// Name of model used when placed on the side of the block
        side_model: Option<ModelConfig>,
        /// The friction or drag.
        friction: Friction,
        /// Which faces the model can cull of adjacent blocks.
        #[serde(default)]
        cull_faces: ModelCullFaces,
        /// If the block is interactable
        #[serde(default)]
        interactable: bool,
    },
}

impl BlockConfig {
    fn from_file(path: &std::path::Path) -> Result<Self, Box<dyn std::error::Error>> {
        let config = Self::read_as_json(path)?;

        return serde_json::from_value(config)
            .map_err(|error| Box::new(error) as Box<dyn std::error::Error>);
    }

    fn read_as_json(
        path: &std::path::Path,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        let file = std::fs::File::open(&path)?;

        let mut config: serde_json::Value = serde_json::from_reader(&file)?;

        // recursively read parent configs
        if let Some(parent) = config["parent"].as_str() {
            let parent_path = std::path::Path::new(BLOCK_CONFIG_PATH)
                .join("parents")
                .join(parent);
            let mut parent: serde_json::Value = match Self::read_as_json(&parent_path) {
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

            // Append to parent to replace the values it shares with the child.
            parent
                .as_object_mut()
                .unwrap()
                .append(&mut config.as_object_mut().unwrap());

            return Ok(parent);
        }

        return Ok(config);
    }
}

// This is derived from the AlphaMode of the block's material as well as the BlockConfig::Cube
// attribute 'cull_self'. 
// 'All' is for AlphaMode::Opaque e.g. stone
// 'OnlyTransparent' for all blending AlphaMode's e.g. water
// 'None' is for AlphaMode::Mask e.g. leaves
// 'OnlySelf' for AlphaMode::Mask with cull_self==true e.g. glass
#[derive(Debug, PartialEq)]
enum CullMethod {
    // Cull all faces that are adjacent.
    All,
    // Cull only other transparent faces that are adjacent. This does not apply to mask
    // transparency. Use for liquids.
    TransparentOnly,
    // Do not cull. Use for blocks with masked transparency, glass, leaves.
    None,
    // Cull only blocks of the same type.
    OnlySelf,
}

#[derive(Debug)]
pub struct QuadPrimitive {
    /// Vertices of the 4 corners of the square.
    pub vertices: [[f32; 3]; 4],
    // XXX: These aren't in use
    // Note that this is necessary to have the two tris angled differently as with water.
    /// Normals for both triangles.
    pub normals: [[f32; 3]; 2],
    /// Index id in the texture array.
    pub texture_array_id: u32,
    /// Which adjacent block face culls this quad from rendering.
    pub cull_face: Option<BlockFace>,
    /// Which blockface this quad will take it's lighting from.
    pub light_face: BlockFace,
}

#[derive(Deserialize)]
struct QuadPrimitiveJson {
    vertices: [[f32; 3]; 4],
    texture: String,
    cull_face: Option<BlockFace>,
}

#[derive(Deserialize)]
struct TextureNames {
    top: String,
    bottom: String,
    left: String,
    right: String,
    front: String,
    back: String,
}

#[derive(Deserialize)]
struct ModelConfig {
    name: String,
    #[serde(default)]
    position: Vec3,
    #[serde(default)]
    rotation: Quat,
}

/// Which faces of the block
#[derive(Default, Debug, Deserialize)]
pub struct ModelCullFaces {
    front: bool,
    back: bool,
    top: bool,
    bottom: bool,
    right: bool,
    left: bool,
}

// The different faces of a block
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BlockFace {
    Top,
    Bottom,
    Right,
    Left,
    Front,
    Back,
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
