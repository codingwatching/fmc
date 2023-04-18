use std::{collections::HashMap, ops::Index, sync::Arc};

use bevy::prelude::*;
use fmc_networking::{messages, BlockId, NetworkClient};
use serde::Deserialize;

use crate::{
    assets,
    rendering::materials::{self, BlockMaterial},
};

pub static BLOCKS: once_cell::sync::OnceCell<Blocks> = once_cell::sync::OnceCell::new();

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
    let mut blocks = HashMap::new();
    let mut block_ids = server_config.block_ids.clone();

    let directory = match std::fs::read_dir(BLOCK_CONFIG_PATH) {
        Ok(dir) => dir,
        Err(e) => {
            net.disconnect(&format!(
                "Misconfigured resource pack, failed to read block configuration directory '{}'\n Error: {}",
                BLOCK_CONFIG_PATH, e)
            );
            return;
        }
    };

    for dir_entry in directory {
        let file_path = match dir_entry {
            Ok(d) => d.path(),
            Err(e) => {
                net.disconnect(&format!(
                    "Misconfigured resource pack, failed to read the file path of a block config\n\
                    Error: {}",
                    e
                ));
                return;
            }
        };

        if file_path.is_dir() {
            continue;
        }

        let block_config = match BlockConfig::from_file(&file_path) {
            Ok(c) => match c {
                Some(c) => c,
                None => continue, // parent config
            },
            Err(e) => {
                net.disconnect(&format!(
                    "Misconfigured resource pack, failed to read block config at {}\nError: {}",
                    file_path.display(),
                    e
                ));
                return;
            }
        };

        let (name, block) = match block_config {
            BlockConfig::Cube {
                name,
                faces,
                cull,
                cull_only_if_same_block,
                quads,
                friction,
                material,
                interactable,
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

                let transparent = match material.alpha_mode {
                    AlphaMode::Blend
                    | AlphaMode::Add
                    | AlphaMode::Multiply
                    | AlphaMode::Mask(_)
                    | AlphaMode::Premultiplied => true,
                    AlphaMode::Opaque => false,
                };

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
                            cull_face: if cull {
                                Some(match i {
                                    0 => BlockFace::Bottom,
                                    1 => BlockFace::Back,
                                    2 => BlockFace::Right,
                                    3 => BlockFace::Left,
                                    4 => BlockFace::Front,
                                    5 => BlockFace::Top,
                                    _ => unreachable!(),
                                })
                            } else {
                                None
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

                        mesh_primitives.push(QuadPrimitive {
                            vertices: quad.vertices,
                            normals,
                            texture_array_id,
                            cull_face: quad.cull_face,
                        });
                    }
                }

                (
                    name.to_owned(),
                    Block::Cube(Cube {
                        name,
                        material_handle,
                        quads: mesh_primitives,
                        friction,
                        cull,
                        cull_only_if_same_block,
                        interactable,
                        transparent,
                    }),
                )
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

                (
                    name.to_owned(),
                    Block::Model(BlockModel {
                        name,
                        center: center_model,
                        side: side_model,
                        friction,
                        cull_faces,
                        interactable,
                    }),
                )
            }
        };

        if let Some(id) = block_ids.remove(&name) {
            blocks.insert(id, block);
        }
    }

    if block_ids.len() > 0 {
        let remaining = block_ids.into_keys().collect::<String>();
        net.disconnect(&format!(
            "Misconfigured resource pack, Missing block configs for blocks with names: {}",
            &remaining
        ));
        return;
    }

    BLOCKS.set(Blocks { inner: blocks }).unwrap();
}

// TODO: Wrap into Blocks(_Blocks), this way it can have 2 get functions. One for the OnceCell and
// one for getting blocks. Just implement deref for blocks. [index] for blocks looks really
// awkward.
// TODO: Convert inner to vec for faster lookup, needs offset or dummy Block at 0 for air, maybe
// have an actual config for it.
/// The configurations for all the blocks.
#[derive(Debug, Default)]
pub struct Blocks {
    pub inner: HashMap<BlockId, Block>,
}

impl Blocks {
    pub fn get() -> &'static Self {
        return BLOCKS
            .get()
            .expect("The blocks have not been loaded yet, make sure this is only used after.");
    }

    pub fn contains(&self, block_id: BlockId) -> bool {
        return block_id as usize <= self.inner.len() && block_id != 0;
    }

    // TODO: Remove this, chunk blocks should be checked at reception for validity.
    pub fn get_config(&self, block_id: &BlockId) -> Option<&Block> {
        return self.inner.get(block_id);
    }
}

impl Index<&BlockId> for Blocks {
    type Output = Block;

    fn index(&self, index: &BlockId) -> &Self::Output {
        return &self.inner[index];
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
    /// If adjacent blocks should cull the side facing this block.
    pub cull: bool,
    // This is useful for things like water, where you want to cull all water blocks adjacent, but
    // nothing else.
    /// If only blocks of this type, facing this block should have their face culled.
    pub cull_only_if_same_block: bool,
    /// If when the player uses their equipped item on this block it should count as an
    /// interaction, or it should count as trying to place its associated block.
    pub interactable: bool,
    /// Marker for if the block is transparent, read from the block's material
    pub transparent: bool,
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
    /// Returns if this block should cull the opposing face of the one provided.
    pub fn culls_face(&self, side: BlockFace) -> bool {
        match self {
            Block::Cube(cube) => cube.cull,
            Block::Model(model) => match side {
                BlockFace::Front => model.cull_faces.front,
                BlockFace::Back => model.cull_faces.back,
                BlockFace::Top => model.cull_faces.top,
                BlockFace::Bottom => model.cull_faces.bottom,
                BlockFace::Left => model.cull_faces.left,
                BlockFace::Right => model.cull_faces.right,
            },
        }
    }

    pub fn only_cull_if_same(&self) -> bool {
        match self {
            Block::Cube(c) => c.cull_only_if_same_block,
            Block::Model(_) => false,
        }
    }

    pub fn is_transparent(&self) -> bool {
        match self {
            Block::Cube(c) => c.transparent,
            Block::Model(_) => false,
        }
    }

    pub fn friction(&self) -> &Friction {
        match self {
            Block::Cube(cube) => &cube.friction,
            Block::Model(model) => &model.friction,
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
        /// If this block should cull all adjacent block faces.
        cull: bool,
        /// If *only* blocks of this type, facing this block should have their face culled.
        #[serde(default)]
        cull_only_if_same_block: bool,
        /// List of quads that make up a mesh.
        quads: Option<Vec<QuadPrimitiveJson>>,
        /// The friction or drag.
        friction: Friction,
        /// Material that should be used to render
        material: String,
        /// If the block is interactable
        #[serde(default)]
        interactable: bool,
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
        /// Which faces the model should cull adjacent blocks.
        #[serde(default)]
        cull_faces: ModelCullFaces,
        /// If the block is interactable
        #[serde(default)]
        interactable: bool,
    },
}

impl BlockConfig {
    fn from_file(path: &std::path::Path) -> Result<Option<Self>, Box<dyn std::error::Error>> {
        let config = Self::read_as_json(path)?;

        // Ignore block configs that don't have an associated name. These are templates to be used
        // as parents for actual blocks.
        // XXX: Will probably cause confusion if someone adds a block and forgets the name.
        if !config["name"].is_string() {
            return Ok(None);
        }

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
}

#[derive(Deserialize)]
struct QuadPrimitiveJson {
    vertices: [[f32; 3]; 4],
    texture: String,
    cull_face: Option<BlockFace>,
    #[serde(default)]
    only_cull_same: bool,
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

// TODO: When the friction is fluid the collidable config has to be false, and vice versa. This has
// to be enforced while reading the config or else it will cause confusion when it causes weird
// error messages.
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
