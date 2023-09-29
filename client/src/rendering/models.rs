use std::collections::HashMap;

use bevy::{
    gltf::Gltf,
    pbr::NotShadowCaster,
    prelude::*,
    render::{mesh::Indices, primitives::Aabb},
};
use fmc_networking::{messages, NetworkData};

use crate::{
    assets::models::Models,
    game_state::GameState,
    player::Player,
    world::{MovesWithOrigin, Origin},
};

pub struct ModelPlugin;
impl Plugin for ModelPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ModelEntities::default()).add_systems(
            Update,
            (
                handle_model_add_delete,
                update_model_asset,
                render_aabb,
                update_transforms,
            )
                .run_if(GameState::in_game),
        );
    }
}

#[derive(Component)]
struct ModelMarker;

/// A map from model id to entity in the ecs
#[derive(Resource, Deref, DerefMut, Default)]
struct ModelEntities(HashMap<u32, Entity>);

fn handle_model_add_delete(
    mut commands: Commands,
    origin: Res<Origin>,
    models: Res<Models>,
    gltf_assets: Res<Assets<Gltf>>,
    mut model_entities: ResMut<ModelEntities>,
    mut deleted_models: EventReader<NetworkData<messages::DeleteModel>>,
    mut new_models: EventReader<NetworkData<messages::NewModel>>,
) {
    for model in deleted_models.read() {
        if let Some(entity) = model_entities.remove(&model.id) {
            // BUG: Every time the model's scene handle changes, a new child entity is attached to
            // this entity. Presumably for the gltf meshes etc. These are not cleaned up when the
            // scene changes. When we call despawn_recursive here it complains about the child
            // entites not existing. Something to do with the hierarchy propagation probably? The
            // gltf stuff is deleted, but a reference to the entity is left hanging in the
            // children.
            warn!("The following warnings (if any) are the result of a bug.");
            commands.entity(entity).despawn_recursive();
        }
    }

    for new_model in new_models.read() {
        let model = if let Some(model) = models.get(&new_model.asset) {
            model
        } else {
            // Disconnect
            todo!();
        };

        // Server may send same id with intent to replace without deleting first
        if let Some(old_entity) = model_entities.remove(&new_model.id) {
            commands.entity(old_entity).despawn_recursive();
        }

        let gltf = gltf_assets.get(&model.handle).unwrap();

        let entity = commands
            .spawn(SceneBundle {
                scene: gltf.scenes[0].clone(),
                transform: Transform {
                    translation: (new_model.position - origin.as_dvec3()).as_vec3(),
                    rotation: new_model.rotation,
                    scale: new_model.scale,
                },
                ..default()
            })
            .insert(MovesWithOrigin)
            .insert(ModelMarker)
            .id();

        model_entities.insert(new_model.id, entity);
    }
}

fn render_aabb(
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    aabb_query: Query<(Entity, &Aabb, &Transform), (With<ModelMarker>, Added<Aabb>)>,
) {
    for (entity, aabb, transform) in aabb_query.iter() {
        let max = aabb.max();
        let min = aabb.min();
        /*
              (2)-----(3)               Y
               | \     | \              |
               |  (1)-----(0) MAX       o---X
               |   |   |   |             \
          MIN (6)--|--(7)  |              Z
                 \ |     \ |
                  (5)-----(4)
        */
        let vertices = vec![
            [max.x, max.y, max.z],
            [min.x, max.y, max.z],
            [min.x, max.y, min.z],
            [max.x, max.y, min.z],
            [max.x, min.y, max.z],
            [min.x, min.y, max.z],
            [min.x, min.y, min.z],
            [max.x, min.y, min.z],
        ];

        let indices = Indices::U32(vec![
            0, 1, 1, 2, 2, 3, 3, 0, // Top
            4, 5, 5, 6, 6, 7, 7, 4, // Bottom
            0, 4, 1, 5, 2, 6, 3, 7, // Verticals
        ]);

        let mut mesh = Mesh::new(bevy::render::render_resource::PrimitiveTopology::LineList);
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vertices.clone());
        mesh.set_indices(Some(indices));

        let child = commands
            .spawn((
                PbrBundle {
                    mesh: meshes.add(mesh),
                    material: materials.add(StandardMaterial {
                        base_color: Color::rgb(0.0, 1.0, 0.0),
                        unlit: true,
                        ..default()
                    }),
                    transform: Transform {
                        scale: 1.0 / transform.scale,
                        translation: Vec3::new(0.0, 0.0, 0.0),
                        ..default()
                    },
                    ..default()
                },
                NotShadowCaster,
            ))
            .id();
        commands.entity(entity).add_child(child);
    }
}

// TODO: This needs interpolation, it's not bad at 60 ticks but the server should run at a lower
// rate probably.
fn update_transforms(
    origin: Res<Origin>,
    model_entities: Res<ModelEntities>,
    mut transform_updates: EventReader<NetworkData<messages::ModelUpdateTransform>>,
    mut model_query: Query<&mut Transform, With<ModelMarker>>,
) {
    for transform_update in transform_updates.read() {
        if let Some(entity) = model_entities.get(&transform_update.id) {
            // I think this should be bug, server should not send model same tick it sends
            // transform updated. But there is 1-frame delay for model entity spawn for command
            // application. Should be disconnect I think, if bevy every gets immediate application
            // of commands.
            let mut transform = match model_query.get_mut(*entity) {
                Ok(m) => m,
                Err(_) => continue,
            };
            transform.translation = (transform_update.position - origin.as_dvec3()).as_vec3();
            transform.rotation = transform_update.rotation;
        }
    }
}

fn update_model_asset(
    model_entities: Res<ModelEntities>,
    models: Res<Models>,
    gltf_assets: Res<Assets<Gltf>>,
    mut asset_updates: EventReader<NetworkData<messages::ModelUpdateAsset>>,
    mut model_query: Query<&mut Handle<Scene>, With<ModelMarker>>,
) {
    for asset_update in asset_updates.read() {
        if let Some(entity) = model_entities.get(&asset_update.id) {
            let mut handle = model_query.get_mut(*entity).unwrap();

            *handle = if let Some(model) = models.get(&asset_update.asset) {
                gltf_assets.get(&model.handle).unwrap().scenes[0].clone()
            } else {
                // Disconnect?
                todo!();
            };
        }
    }
}
