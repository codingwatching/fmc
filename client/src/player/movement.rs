// TODO: This needs a lot of refinement. Bobbing while walking. Jumping feels floaty. Bobbing on
// the water is too sharp. Falling speed is too slow, but while jumping you fall too fast.

use bevy::{
    math::Vec3A,
    prelude::*,
    render::primitives::Aabb,
    window::{CursorGrabMode, PrimaryWindow},
};
use fmc_networking::{messages, NetworkClient, NetworkData};

use crate::{
    game_state::GameState,
    player::Player,
    world::{
        blocks::{Blocks, Friction},
        world_map::WorldMap,
        Origin,
    },
};

// sqrt(2 * gravity * wanted height(1.4)) + some for air resistance that I don't bother calculating
const JUMP_VELOCITY: f32 = 9.5;
const GRAVITY: Vec3 = Vec3::new(0.0, -32.0, 0.0);
// This is needed so that whenever you land early you can't just instantly jump again.
// v_t = v_0 * at => (v_t - v_0) / a = t
const JUMP_TIME: f32 = JUMP_VELOCITY * 1.5 / -GRAVITY.y;

pub struct MovementPlugin;
impl Plugin for MovementPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                toggle_flight,
                change_player_velocity,
                simulate_player_physics.after(change_player_velocity),
            )
                .run_if(in_state(GameState::Playing)),
        )
        // TODO: This is another one of the things the server just sends on connection.
        // Workaround by just having it run all the time, but once the server can be notified
        // that the client is actually ready to receive it should be moved above with the rest.
        .add_systems(Update, handle_position_updates_from_server);
    }
}

#[derive(Deref)]
struct Timer {
    pub last: std::time::Instant,
}

impl Default for Timer {
    fn default() -> Self {
        return Self {
            last: std::time::Instant::now(),
        };
    }
}

fn handle_position_updates_from_server(
    origin: Res<Origin>,
    mut position_events: EventReader<NetworkData<messages::PlayerPosition>>,
    mut player_query: Query<&mut Transform, With<Player>>,
) {
    for event in position_events.iter() {
        let mut transform = player_query.single_mut();
        transform.translation = (event.position - origin.as_dvec3()).as_vec3();
    }
}

// TODO: Hack until proper input handling, note pressing fast three times will put you back into
// the original state.
fn toggle_flight(
    keys: Res<Input<KeyCode>>,
    mut query: Query<&mut Player>,
    mut timer: Local<Timer>,
) {
    for key in keys.get_just_released() {
        if KeyCode::Space == *key {
            if std::time::Instant::now()
                .duration_since(timer.last)
                .as_millis()
                < 250
            {
                let mut player = query.single_mut();
                player.flying = !player.flying;
                player.velocity = Vec3::ZERO;
            } else {
                timer.last = std::time::Instant::now();
            }
        }
    }
}

// TODO: This blends moving and flying movement, they should be split in separate systems
/// Handles keyboard input and movement
fn change_player_velocity(
    keys: Res<Input<KeyCode>>,
    window: Query<&Window, With<PrimaryWindow>>,
    mut player_query: Query<&mut Player>,
    camera_query: Query<&Transform, With<Camera>>,
    mut last_jump: Local<Timer>,
) {
    let mut player = player_query.single_mut();
    let camera_transform = camera_query.single();

    let window = window.single();

    let camera_forward = camera_transform.forward();
    let forward = Vec3::new(camera_forward.x, 0., camera_forward.z);
    let right = Vec3::new(-camera_forward.z, 0., camera_forward.x);

    let mut horizontal_velocity = Vec3::ZERO;
    let mut vertical_velocity = Vec3::ZERO;
    for key in keys.get_pressed() {
        if window.cursor.grab_mode == CursorGrabMode::Locked {
            match key {
                KeyCode::W => horizontal_velocity += forward,
                KeyCode::S => horizontal_velocity -= forward,
                KeyCode::A => horizontal_velocity -= right,
                KeyCode::D => horizontal_velocity += right,
                KeyCode::Space => {
                    if player.is_grounded.y && last_jump.elapsed().as_secs_f32() > JUMP_TIME
                        || player.flying
                        || player.swimming
                    {
                        last_jump.last = std::time::Instant::now();
                        vertical_velocity += Vec3::Y
                    }
                }
                KeyCode::LShift => {
                    if player.flying || player.swimming {
                        vertical_velocity -= Vec3::Y
                    }
                }

                _ => (),
            }
        }
    }

    if horizontal_velocity != Vec3::ZERO {
        horizontal_velocity = horizontal_velocity.normalize();
    }

    if player.flying && keys.pressed(KeyCode::LControl) {
        horizontal_velocity *= 10.0;
    }

    if player.flying {
        horizontal_velocity *= 11.0;
    } else if player.is_grounded.y {
        horizontal_velocity *= 4.3;
    } else {
        horizontal_velocity *= 4.3;
    }

    vertical_velocity.y = vertical_velocity.y * JUMP_VELOCITY;

    let mut velocity = horizontal_velocity + vertical_velocity;

    // Only change the player velocity if it is less than the new velocity.
    // i.e. You should only be able to add to the velocity
    if !player.flying {
        velocity = Vec3::select(
            player.velocity.abs().cmpgt(velocity.abs()),
            player.velocity,
            velocity,
        );
    }

    if !velocity.is_nan() {
        player.velocity = velocity;
    }
}

// TODO: This needs to be timestepped. When you unfocus the window it slows down tick rate, makes
// you tunnel.
// TODO: If you travel more than 0.5 blocks per tick you will tunnel.
fn simulate_player_physics(
    origin: Res<Origin>,
    world_map: Res<WorldMap>,
    time: Res<Time>,
    net: Res<NetworkClient>,
    mut player: Query<(&mut Player, &mut Transform, &Aabb)>,
    mut last_position_sent_to_server: Local<Vec3>,
) {
    let (mut player, mut transform, player_aabb) = player.single_mut();

    if player.velocity.x != 0.0 {
        player.is_grounded.x = false;
    }
    if player.velocity.y != 0.0 {
        player.is_grounded.y = false;
    }
    if player.velocity.z != 0.0 {
        player.is_grounded.z = false;
    }

    if !player.flying {
        player.velocity += GRAVITY * time.delta_seconds();
    }

    // TODO: Maybe until Fixed time step run criteria it can do delta_seconds/ms_per_update
    // iterations
    let pos_after_move = transform.translation + player.velocity * time.delta_seconds();

    let player_aabb = Aabb {
        center: player_aabb.center + Vec3A::from(pos_after_move),
        half_extents: player_aabb.half_extents,
    };

    // Check for collisions for all blocks within the player's aabb.
    let mut collisions = Vec::new();
    let start = player_aabb.min().floor().as_ivec3() + origin.0;
    let stop = player_aabb.max().floor().as_ivec3() + origin.0;
    for x in start.x..=stop.x {
        for y in start.y..=stop.y {
            for z in start.z..=stop.z {
                let block_pos = IVec3::new(x, y, z);

                let block_id = match world_map.get_block(&block_pos) {
                    Some(id) => id,
                    // Disconnect? Should always have your surroundings loaded.
                    None => continue,
                };

                let block_aabb = Aabb {
                    center: (block_pos - origin.0).as_vec3a() + 0.5,
                    half_extents: Vec3A::new(0.5, 0.5, 0.5),
                };

                let overlap = player_aabb.half_extents + block_aabb.half_extents
                    - (player_aabb.center - block_aabb.center).abs();

                if overlap.cmpgt(Vec3A::ZERO).all() {
                    collisions.push((Vec3::from(overlap), block_id));
                }
            }
        }
    }

    let velocity = player.velocity;
    let mut friction = Vec3::ZERO;
    let mut move_back = Vec3::ZERO;
    let delta_time = Vec3::splat(time.delta_seconds());

    let blocks = Blocks::get();

    for (collision, block_id) in collisions {
        let backwards_time = collision / velocity.abs();
        // Small epsilon to delta time because precision is off for unknown reason.
        // TODO: This delta causes a lot of trouble, too high and you clip where you shouldn't, too
        // low you glitch through floor. Move to f64 probably.
        let valid_axes = backwards_time.cmplt(delta_time + 0.00001);
        let slowest_resolution_axis =
            Vec3::select(valid_axes, backwards_time, Vec3::NAN).max_element();

        match blocks[&block_id].friction() {
            Friction::Static {
                front,
                back,
                right,
                left,
                top,
                bottom,
            } => {
                if slowest_resolution_axis == backwards_time.x {
                    move_back.x =
                        backwards_time.x * -velocity.x + (-velocity.x).signum() * f32::EPSILON;
                    player.velocity.x = 0.0;
                    player.is_grounded.x = true;

                    if velocity.x.is_sign_positive() {
                        friction = friction.max(Vec3::splat(*left));
                    } else {
                        friction = friction.max(Vec3::splat(*right));
                    }
                } else if slowest_resolution_axis == backwards_time.y {
                    move_back.y =
                        backwards_time.y * -velocity.y + (-velocity.y).signum() * f32::EPSILON;
                    player.velocity.y = 0.0;
                    player.is_grounded.y = true;

                    if velocity.y.is_sign_positive() {
                        friction = friction.max(Vec3::splat(*bottom));
                    } else {
                        friction = friction.max(Vec3::splat(*top));
                    }
                } else if slowest_resolution_axis == backwards_time.z {
                    move_back.z =
                        backwards_time.z * -velocity.z + (-velocity.z).signum() * f32::EPSILON;
                    player.velocity.z = 0.0;
                    player.is_grounded.z = true;

                    if velocity.z.is_sign_positive() {
                        friction = friction.max(Vec3::splat(*back));
                    } else {
                        friction = friction.max(Vec3::splat(*front));
                    }
                }
            }
            Friction::Drag(drag) => {
                friction = friction.max(*drag);
            }
        }
    }

    // TODO: This is just some random value that is larger than air. Thinking maybe blocks should
    // have a climbable property instead of relying on "density".
    player.swimming = !player.is_grounded.y && friction.y > 0.05;

    transform.translation = pos_after_move + move_back;
    // TODO: Pow(20) is a way to scale. I think it is not linear and I think maybe it should be, idk too tired.
    player.velocity = player.velocity * (1.0 - friction).powf(20.0).powf(time.delta_seconds());

    // Avoid sending constant position updates to the server.
    if (*last_position_sent_to_server - transform.translation)
        .abs()
        .cmpgt(Vec3::splat(0.01))
        .any()
    {
        *last_position_sent_to_server = transform.translation;
        net.send_message(messages::PlayerPosition {
            position: transform.translation.as_dvec3() + origin.as_dvec3(),
        });
    }
}
