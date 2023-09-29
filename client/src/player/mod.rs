use bevy::{prelude::*, render::primitives::Aabb};
use fmc_networking::{messages, NetworkData};

use crate::{game_state::GameState, settings::Settings, world::MovesWithOrigin};

mod camera;
// TODO: This is pub because of asset loading, remove when redone
mod movement;
mod physics;

pub use camera::PlayerCameraMarker;

// Used at setup to set camera position and define the AABB, but should be changed by the server.
const DEFAULT_PLAYER_WIDTH: f32 = 0.6;
const DEFAULT_PLAYER_HEIGHT: f32 = 1.8;

/// Contains everything needed to add first-person fly camera behavior to your game
pub struct PlayerPlugin;
impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(movement::MovementPlugin)
            .add_plugins(camera::CameraPlugin)
            .add_systems(Startup, setup_player)
            .add_systems(
                Update,
                handle_player_config.run_if(GameState::in_game),
            );
    }
}

// TODO: All this physics/control stuff has no business here. Server should send wasm plugin that
// does everything. This is needed for other types of movement too, like boats.
#[derive(Component, Default)]
pub struct Player {
    // Current velocity
    pub velocity: Vec3,
    // Current acceleration
    pub acceleration: Vec3,
    pub is_flying: bool,
    pub is_swimming: bool,
    // If the player is against a block. (in any direction)
    pub is_grounded: BVec3,
}

impl Player {
    pub fn new() -> Self {
        return Self {
            is_flying: true,
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

    let camera_entity = commands
        .spawn((camera::CameraBundle::default(), settings.fog.clone()))
        .id();

    let player_entity = commands
        .spawn(player)
        // XXX: I did not want this, but it is required for visibility by the visibility
        // propagation system.
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
    for config in config_events.read() {
        let mut aabb = aabb_query.single_mut();
        let mut camera_transform = camera_query.single_mut();

        camera_transform.translation = config.camera_position;

        *aabb = Aabb::from_min_max(Vec3::ZERO, config.aabb_dimensions)
    }
}
