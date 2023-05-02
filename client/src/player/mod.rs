// Copied from https://github.com/sburris0/bevy_flycam/blob/master/src/lib.rs
use bevy::{core_pipeline::prepass::DepthPrepass, prelude::*, render::primitives::Aabb};
use fmc_networking::{messages, NetworkData};

use crate::{
    constants::CHUNK_SIZE, game_state::GameState, settings::Settings, world::MovesWithOrigin,
};

mod camera;
mod hand;
pub mod interfaces;
pub mod key_bindings;
mod movement;
mod physics;

// Used at setup to set camera position and define the AABB, but should be changed by the server.
const DEFAULT_PLAYER_WIDTH: f32 = 0.6;
const DEFAULT_PLAYER_HEIGHT: f32 = 1.8;

/// Contains everything needed to add first-person fly camera behavior to your game
pub struct PlayerPlugin;
impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugin(movement::MovementPlugin)
            .add_plugin(interfaces::InterfacePlugin)
            .add_plugin(hand::HandPlugin)
            .add_plugin(key_bindings::KeyBindingsPlugin)
            .add_systems(Startup, setup_player)
            .add_systems(
                Update,
                (camera::camera_rotation, handle_player_config)
                    .run_if(in_state(GameState::Playing)),
            );
    }
}

// TODO: All this physics/control stuff has no business here. Server should send wasm plugin that
// does everything. This is needed for other types of movement too, like boats.
#[derive(Component, Default)]
pub struct Player {
    // Current velocity of the player
    pub velocity: Vec3,
    pub flying: bool,
    pub swimming: bool,
    // If the player is against a block. (in any direction)
    pub is_grounded: BVec3,
    // Vertical angle of the camera.
    pub pitch: f32,
    /// Horizonal angle of the camera.
    pub yaw: f32,
}

impl Player {
    pub fn new() -> Self {
        return Self {
            flying: true,
            ..Default::default()
        };
    }
}

fn setup_player(mut commands: Commands, settings: Res<Settings>) {
    let player = Player::new();
    // TODO: The server should be able to define this so that you can play as different sized
    // things.
    let aabb = Aabb::from_min_max(
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(
            DEFAULT_PLAYER_WIDTH,
            DEFAULT_PLAYER_HEIGHT,
            DEFAULT_PLAYER_WIDTH,
        ),
    );

    let hand_entity = hand::hand_setup(&mut commands);

    let camera_entity = commands
        .spawn(Camera3dBundle {
            transform: Transform::from_xyz(
                DEFAULT_PLAYER_WIDTH / 2.0,
                DEFAULT_PLAYER_HEIGHT,
                DEFAULT_PLAYER_WIDTH / 2.0,
            ),
            projection: PerspectiveProjection {
                // TODO: Don't remember why this was necessary, I think it limits the frustum
                far: settings.render_distance as f32 * CHUNK_SIZE as f32,
                ..default()
            }
            .into(),
            ..default()
        })
        .insert(DepthPrepass)
        .insert(camera::PlayerCameraMarker)
        .insert(camera::CameraState::default())
        // XXX: Remove in future if requirement for parent to have is removed. Needed for equipped
        // item
        .insert(VisibilityBundle::default())
        .add_child(hand_entity)
        .id();

    let player_entity = commands
        .spawn(player)
        // XXX: I did not want this, but it is required for visibility by the visibility
        // propagation system. Requirement will be removed later by bevy I think. Version 0.8 when
        // I type this.
        .insert(VisibilityBundle::default())
        .insert(TransformBundle {
            local: Transform::from_translation(Vec3::NAN),
            ..default()
        })
        .insert(MovesWithOrigin)
        .insert(aabb)
        .id();

    commands
        .entity(player_entity)
        .push_children(&[camera_entity]);
}

// TODO: The config event is sometimes missed says bevy. Probably because the sever sends it on
// connection, and we don't enter GameState::Playing before we've finished setup.
// Server defines some aspects about the player at startup (but can be changed), they have
// defaults, but should be updated by the server on connection.
fn handle_player_config(
    mut config_events: EventReader<NetworkData<messages::PlayerConfiguration>>,
    mut aabb_query: Query<&mut Aabb, With<Player>>,
    mut camera_query: Query<&mut Transform, With<Camera>>,
) {
    for config in config_events.iter() {
        let mut aabb = aabb_query.single_mut();
        let mut camera_transform = camera_query.single_mut();

        camera_transform.translation = config.camera_position;

        *aabb = Aabb::from_min_max(Vec3::ZERO, config.aabb_dimensions)
    }
}
