use std::{collections::HashMap, time::Duration};

use bevy::{gltf::Gltf, prelude::*};

use fmc_networking::{messages::ServerConfig, NetworkClient};

const MODEL_PATH: &str = "server_assets/textures/models/";

pub type ModelId = u32;

/// A map from server model id to asset handle
#[derive(Resource)]
pub struct Models {
    // id -> model
    inner: HashMap<ModelId, Model>,
    // filename -> id
    reverse: HashMap<String, ModelId>,
}

impl Models {
    pub fn get(&self, id: &u32) -> Option<&Model> {
        return self.inner.get(&id);
    }

    pub fn get_id_by_filename(&self, filename: &str) -> Option<u32> {
        return self.reverse.get(filename).cloned();
    }

    pub fn iter(&self) -> std::collections::hash_map::Values<u32, Model> {
        return self.inner.values();
    }
}

// TODO: Idk why I made this struct, remove if it isn't expanded upon
pub struct Model {
    pub handle: Handle<Gltf>,
}

// TODO: If AssetServer implements some kind of synchronous load or verification in the future,
// models should be confirmed so we can disconnect when a model fails to load.
pub(super) fn load_models(
    mut commands: Commands,
    net: Res<NetworkClient>,
    server_config: Res<ServerConfig>,
    asset_server: Res<AssetServer>,
) {
    let mut models = Models {
        inner: HashMap::new(),
        reverse: HashMap::new(),
    };

    let directory = match std::fs::read_dir(MODEL_PATH) {
        Ok(dir) => dir,
        Err(e) => {
            net.disconnect(&format!(
                "Misconfigured resource pack: Failed to read model directory at '{}'\n Error: {}",
                MODEL_PATH, e
            ));
            return;
        }
    };

    // We first load all the models, they can be in different formats so it's simpler to extract
    // the names from the file entries that try to add an extension to the name sent by the server.
    let mut handles: HashMap<String, Handle<Gltf>> = HashMap::new();
    for dir_entry in directory {
        let file_path = match dir_entry {
            Ok(d) => d.path(),
            Err(e) => {
                net.disconnect(&format!(
                    "Misconfigured resource pack: Failed to read the file path of a model\n\
                    Error: {}",
                    e
                ));
                return;
            }
        };

        let model_name = file_path
            .file_stem()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let model_handle = asset_server.load(file_path);

        handles.insert(model_name, model_handle);
    }

    for (name, id) in server_config.model_ids.iter() {
        if let Some(handle) = handles.remove(name) {
            models.reverse.insert(name.to_owned(), *id);
            models.inner.insert(
                *id,
                Model {
                    handle,
                },
            );
        } else {
            net.disconnect(&format!(
                "Misconfigured resource pack: Missing model, no model with the name '{}'",
                name
            ));
        }
    }

    commands.insert_resource(models);
}
