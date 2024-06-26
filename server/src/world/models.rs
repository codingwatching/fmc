use std::{
    collections::{HashMap, HashSet},
    io::{BufReader, Read},
};

use bevy::{math::DVec3, prelude::*};
use fmc_networking::{messages, ConnectionId, NetworkServer};

use crate::{
    bevy_extensions::f64_transform::{F64GlobalTransform, F64Transform},
    database::Database,
    physics::shapes::Aabb,
    utils,
    world::world_map::chunk_manager::{ChunkSubscriptions, SubscribeToChunk},
};

// TODO:
//use super::world_map::chunk_manager::ChunkUnloadEvent;

pub const MODEL_PATH: &str = "./resources/client/textures/models/";

pub type ModelId = u32;

pub struct ModelPlugin;
impl Plugin for ModelPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ModelMap::default())
            .add_systems(PreStartup, load_models)
            .add_systems(
                Update,
                (
                    send_models_on_chunk_subscription,
                    update_model_transforms,
                    update_model_assets,
                    update_visibility,
                ),
            )
            // TODO: Maybe all of these systems should be PostUpdate. This way Update is the do
            // things place, and PostUpdate is the send to client place.
            //
            // XXX: PostUpdate because RemovedComponents is only available from the stage it was
            // removed up to CoreStage::Last.
            .add_systems(PostUpdate, remove_models);
    }
}

fn load_models(mut commands: Commands, database: Res<Database>) {
    let ids = database.load_model_ids();

    let mut to_check = ids.clone();
    let mut configs = HashMap::with_capacity(ids.len());

    let directory = std::fs::read_dir(MODEL_PATH).expect(&format!(
        "Could not read files from model directory, make sure it is present at '{}'.",
        &MODEL_PATH
    ));

    for dir_entry in directory {
        let path = match dir_entry {
            Ok(d) => d.path(),
            Err(e) => panic!("Failed to read the filename of a model, Error: {}", e),
        };

        // TODO: all unwraps can be invalid
        let Some(id) = to_check.remove(&path.file_stem().unwrap().to_str().unwrap().to_lowercase())
        else {
            continue;
        };

        let Some(extension) = path.extension() else {
            panic!(
                "Invalid model file at '{}', the file is missing an extension",
                path.display()
            )
        };

        if extension == "json" {
            configs.insert(
                id,
                ModelConfig {
                    aabb: Aabb::from_min_max(DVec3::ZERO, DVec3::ONE),
                },
            );
        } else if extension == "glb" || extension == "gltf" {
            let mut reader = match std::fs::File::open(&path) {
                Ok(f) => BufReader::new(f),
                Err(e) => panic!("Failed to open model at: {}\nError: {}", path.display(), e),
            };

            // Skip magic and header
            reader.seek_relative(12).unwrap();

            // Length of json
            let mut buf = [0u8; 4];
            reader.read_exact(&mut buf).unwrap();
            let length = u32::from_le_bytes(buf);

            // TODO: It will just fail here if it isn't correct. Should do a complete validation of
            // the assets so that the clients won't encounter malformed assets.
            let mut buf = vec![0u8; length as usize + 4];
            reader.read_exact(&mut buf).unwrap();
            // Skip 'JSON' prefix.
            let gltf: serde_json::Value = serde_json::from_slice(&buf[4..]).unwrap();

            let mut min = DVec3::splat(f64::MAX);
            let mut max = DVec3::splat(f64::MIN);

            for accessor in gltf["accessors"].as_array().unwrap().iter() {
                let accessor_min = match accessor.get("min") {
                    Some(serde_json::Value::Array(min)) if min.len() == 3 => min,
                    _ => continue,
                };
                let accessor_max = match accessor.get("max") {
                    Some(serde_json::Value::Array(max)) if max.len() == 3 => max,
                    _ => continue,
                };

                for i in 0..3 {
                    min[i] = min[i].min(accessor_min[i].as_f64().unwrap());
                    max[i] = max[i].max(accessor_max[i].as_f64().unwrap());
                }
            }

            configs.insert(
                id,
                ModelConfig {
                    aabb: Aabb::from_min_max(min, max),
                },
            );
        } else {
            panic!("Invalid ")
        }
    }

    if to_check.len() != 0 {
        panic!(
            "Failed to load models. Some models are missing from '{}': {:?}",
            MODEL_PATH,
            to_check.keys()
        );
    }

    commands.insert_resource(Models { ids, configs });
}

#[derive(Bundle)]
pub struct ModelBundle {
    pub model: Model,
    pub visibility: ModelVisibility,
    pub global_transform: F64GlobalTransform,
    pub transform: F64Transform,
}

#[derive(Component)]
pub struct Model {
    pub asset_id: ModelId,
    pub idle_animation_id: Option<u32>,
    pub moving_animation_id: Option<u32>,
}

impl Model {
    pub fn new(id: u32) -> Self {
        return Self {
            asset_id: id,
            idle_animation_id: None,
            moving_animation_id: None,
        };
    }
}

#[derive(Component)]
pub struct ModelVisibility {
    pub is_visible: bool,
}

impl Default for ModelVisibility {
    fn default() -> Self {
        Self { is_visible: true }
    }
}

pub struct ModelConfig {
    pub aabb: Aabb,
}

// TODO: Convert to OnceCell?
#[derive(Resource)]
pub struct Models {
    // A map from asset filename(without extenstion) to model id.
    ids: HashMap<String, u32>,
    // TODO: This can be converted to a vec
    configs: HashMap<u32, ModelConfig>,
}

impl Models {
    pub fn get(&self, id: &u32) -> &ModelConfig {
        self.configs.get(id).unwrap()
    }

    #[track_caller]
    pub fn get_id(&self, name: &str) -> u32 {
        return match self.ids.get(name) {
            Some(a) => *a,
            None => panic!("There is no model with the name: {}", name),
        };
    }

    pub fn clone_ids(&self) -> HashMap<String, u32> {
        return self.ids.clone();
    }
}

/// Keeps track of which chunk every entity with a model is currently in.
#[derive(Default, Resource)]
pub struct ModelMap {
    models: HashMap<IVec3, HashSet<Entity>>,
    reverse: HashMap<Entity, IVec3>,
}

impl ModelMap {
    pub fn get_entities(&self, chunk_position: &IVec3) -> Option<&HashSet<Entity>> {
        return self.models.get(chunk_position);
    }

    fn insert_or_move(&mut self, chunk_position: IVec3, entity: Entity) {
        if let Some(current_chunk_pos) = self.reverse.get(&entity) {
            // Move model from one chunk to another
            if current_chunk_pos == &chunk_position {
                return;
            } else {
                let past_chunk_pos = self.reverse.remove(&entity).unwrap();

                self.models
                    .get_mut(&past_chunk_pos)
                    .unwrap()
                    .remove(&entity);

                self.models
                    .entry(chunk_position)
                    .or_insert(HashSet::new())
                    .insert(entity);

                self.reverse.insert(entity, chunk_position);
            }
        } else {
            // First time seeing model, insert it normally
            self.models
                .entry(chunk_position)
                .or_insert(HashSet::new())
                .insert(entity);
            self.reverse.insert(entity, chunk_position);
        }
    }
}

fn remove_models(
    net: Res<NetworkServer>,
    mut model_map: ResMut<ModelMap>,
    chunk_subscriptions: Res<ChunkSubscriptions>,
    mut deleted_models: RemovedComponents<Model>,
) {
    for entity in deleted_models.read() {
        let chunk_pos = if let Some(position) = model_map.reverse.remove(&entity) {
            model_map.models.get_mut(&position).unwrap().remove(&entity);
            position
        } else {
            // TODO: This if condition can be removed, I just want to test for a while that I didn't
            // mess up.
            panic!("All models that are created should be entered into the model map. \
                   If when trying to delete a model it doesn't exist in the model map that is big bad.")
        };

        if let Some(subs) = chunk_subscriptions.get_subscribers(&chunk_pos) {
            net.send_many(subs, messages::DeleteModel { id: entity.index() });
        }
    }
}

// TODO: Split position, rotation and scale into packets?
fn update_model_transforms(
    net: Res<NetworkServer>,
    chunk_subscriptions: Res<ChunkSubscriptions>,
    mut model_map: ResMut<ModelMap>,
    model_query: Query<
        (Entity, &F64GlobalTransform, &ModelVisibility, Ref<Model>),
        Changed<F64GlobalTransform>,
    >,
) {
    for (entity, global_transform, visibility, tracker) in model_query.iter() {
        let transform = global_transform.compute_transform();
        let chunk_pos = utils::world_position_to_chunk_position(transform.translation.as_ivec3());

        model_map.insert_or_move(chunk_pos, entity);

        if !visibility.is_visible || tracker.is_added() {
            continue;
        }

        let subs = match chunk_subscriptions.get_subscribers(&chunk_pos) {
            Some(subs) => subs,
            None => continue,
        };

        net.send_many(
            subs,
            messages::ModelUpdateTransform {
                id: entity.index(),
                position: transform.translation,
                rotation: transform.rotation.as_f32(),
                scale: transform.scale.as_vec3(),
            },
        );
    }
}

fn update_model_assets(
    net: Res<NetworkServer>,
    chunk_subscriptions: Res<ChunkSubscriptions>,
    model_query: Query<(Entity, Ref<Model>, &F64Transform, &ModelVisibility), Changed<Model>>,
) {
    for (entity, model, transform, visibility) in model_query.iter() {
        if !visibility.is_visible || model.is_added() {
            continue;
        }

        let chunk_pos = utils::world_position_to_chunk_position(transform.translation.as_ivec3());

        let subs = match chunk_subscriptions.get_subscribers(&chunk_pos) {
            Some(subs) => subs,
            None => continue,
        };

        net.send_many(
            subs,
            messages::ModelUpdateAsset {
                id: entity.index(),
                asset: model.asset_id,
                idle_animation: model.idle_animation_id,
                moving_animation: model.moving_animation_id,
            },
        );
    }
}

fn update_visibility(
    net: Res<NetworkServer>,
    chunk_subscriptions: Res<ChunkSubscriptions>,
    model_query: Query<
        (Entity, &Model, &ModelVisibility, &F64GlobalTransform),
        Changed<ModelVisibility>,
    >,
) {
    for (entity, model, visibility, transform) in model_query.iter() {
        let transform = transform.compute_transform();

        let chunk_pos = utils::world_position_to_chunk_position(transform.translation.as_ivec3());

        let subs = match chunk_subscriptions.get_subscribers(&chunk_pos) {
            Some(subs) => subs,
            None => continue,
        };

        if visibility.is_visible {
            net.send_many(
                subs,
                messages::NewModel {
                    parent_id: None,
                    id: entity.index(),
                    asset: model.asset_id,
                    position: transform.translation,
                    rotation: transform.rotation.as_f32(),
                    scale: transform.scale.as_vec3(),
                    idle_animation: model.idle_animation_id,
                    moving_animation: model.moving_animation_id,
                },
            );
        } else {
            net.send_many(subs, messages::DeleteModel { id: entity.index() });
        }
    }
}

fn send_models_on_chunk_subscription(
    net: Res<NetworkServer>,
    model_map: Res<ModelMap>,
    player_query: Query<&ConnectionId>,
    models: Query<(
        Option<&Parent>,
        &Model,
        &F64GlobalTransform,
        &ModelVisibility,
    )>,
    mut chunk_sub_events: EventReader<SubscribeToChunk>,
) {
    for chunk_sub in chunk_sub_events.read() {
        if let Some(model_entities) = model_map.get_entities(&chunk_sub.chunk_position) {
            for entity in model_entities.iter() {
                let Ok((maybe_player_parent, model, transform, visibility)) = models.get(*entity)
                else {
                    continue;
                };

                if !visibility.is_visible {
                    continue;
                }

                // Don't send the player models to the players they belong to.
                if let Some(parent) = maybe_player_parent {
                    let connection_id = player_query.get(parent.get()).unwrap();
                    if connection_id == &chunk_sub.connection_id {
                        continue;
                    }
                }

                let transform = transform.compute_transform();

                net.send_one(
                    chunk_sub.connection_id,
                    messages::NewModel {
                        id: entity.index(),
                        parent_id: None,
                        position: transform.translation,
                        rotation: transform.rotation.as_f32(),
                        scale: transform.scale.as_vec3(),
                        asset: model.asset_id,
                        idle_animation: model.idle_animation_id,
                        moving_animation: model.moving_animation_id,
                    },
                );
            }
        }
    }
}
