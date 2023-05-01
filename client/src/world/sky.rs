// TODO: The sky should be fully defined by the server, the client should get nothing but position
// updates for celestial objects.

use bevy::{
    pbr::{NotShadowCaster, NotShadowReceiver},
    prelude::*,
};
use fmc_networking::{messages, NetworkData};

use crate::{game_state::GameState, player::Player, rendering::materials};

pub const SUN_DISTANCE: f32 = 400000.0;

pub struct SkyPlugin;
impl Plugin for SkyPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            OnEnter(GameState::Playing),
            // This is just a hacky way to have it run after player setup, all of this should
            // be removed when the sky is defined by the server.
            setup,
        )
        .add_systems(Update, pass_time.run_if(in_state(GameState::Playing)));
    }
}

fn setup(
    mut commands: Commands,
    player_query: Query<Entity, With<Player>>,
    mut sky_materials: ResMut<Assets<materials::SkyMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let player_id = player_query.single();

    let sky_entity = commands
        .spawn(MaterialMeshBundle {
            mesh: meshes.add(
                Mesh::try_from(shape::Icosphere {
                    radius: 4900.0,
                    subdivisions: 5,
                })
                .unwrap(),
            ),
            material: sky_materials.add(materials::SkyMaterial::default()),
            ..Default::default()
        })
        .insert(NotShadowCaster)
        .insert(NotShadowReceiver)
        .id();

    commands.insert_resource(AmbientLight {
            color: Color::WHITE,
            brightness: 0.03,
    });

    // Overlays a DirectionalLight on top of the sun that is generated in the shader, since that
    // one doesn't actually illuminate anything.
    // commands.insert_resource(DirectionalLightShadowMap { size: 4096 });
    let sun_entity = commands
        .spawn(DirectionalLightBundle {
            directional_light: DirectionalLight {
                illuminance: 10000.0,
                shadows_enabled: true,
                ..default()
            },
            ..default()
        })
        .id();

    commands
        .entity(player_id)
        .push_children(&[sun_entity, sky_entity]);
}

fn pass_time(
    sky_material_query: Query<&Handle<materials::SkyMaterial>>,
    mut sun_light_query: Query<(&mut Transform, &mut DirectionalLight)>,
    mut materials: ResMut<Assets<materials::SkyMaterial>>,
    mut server_time_events: EventReader<NetworkData<messages::Time>>,
) {
    let angle = if let Some(t) = server_time_events.iter().last() {
        t.angle
    } else {
        return;
    };

    let (mut light_transform, mut light) = sun_light_query.single_mut();

    // Sun goes in a circle around the player
    let position = Vec3::new(angle.cos() * 500., angle.sin() * 500., 0.0);

    light.illuminance = (angle.sin() * 10000.0).max(0.0);

    light_transform.translation = position;
    light_transform.look_at(Vec3::ZERO, Vec3::Y);

    let handle = sky_material_query.single();
    let material = materials.get_mut(handle).unwrap();

    let position = Vec3::new(angle.cos() * SUN_DISTANCE, angle.sin() * SUN_DISTANCE, 0.0);

    material.sun_position.x = position.x;
    material.sun_position.y = position.y;
    material.sun_position.z = position.z;
}
