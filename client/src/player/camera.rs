use bevy::{
    input::mouse::MouseMotion,
    prelude::*,
    window::{CursorGrabMode, PrimaryWindow},
};

use fmc_networking::{messages, NetworkClient, NetworkData};

use crate::settings::Settings;

#[derive(Component, Default)]
pub(super) struct CameraState {
    /// Vertical angle
    pub pitch: f32,
    /// Horizontal angle
    pub yaw: f32,
}

#[derive(Component)]
pub struct PlayerCameraMarker;

/// Handles looking around if cursor is locked
pub(super) fn camera_rotation(
    window: Query<&Window, With<PrimaryWindow>>,
    settings: Res<Settings>,
    net: Res<NetworkClient>,
    mut mouse_events: EventReader<MouseMotion>,
    mut camera_query: Query<(&mut CameraState, &mut Transform), With<Camera>>,
) {
    let window = window.single();

    // Mouse in use by some interface
    if window.cursor.visible == true {
        return;
    }

    // It empties the iterator so it can't access it after loop.
    let should_send = mouse_events.len() > 0;

    for ev in mouse_events.iter() {
        let (mut camera_state, mut transform) = camera_query.single_mut();

        if window.cursor.grab_mode == CursorGrabMode::Locked {
            camera_state.pitch -=
                (settings.sensitivity * ev.delta.y * window.height()).to_radians();
            camera_state.yaw -= (settings.sensitivity * ev.delta.x * window.width()).to_radians();
        }

        camera_state.pitch = camera_state.pitch.clamp(-1.54, 1.54);

        transform.rotation = Quat::from_axis_angle(Vec3::Y, camera_state.yaw)
            * Quat::from_axis_angle(Vec3::X, camera_state.pitch);
    }

    if should_send {
        net.send_message(messages::PlayerCameraRotation {
            rotation: camera_query.single().1.rotation,
        })
    }
}

// Forced camera rotation by the server.
pub(super) fn handle_camera_rotation_from_server(
    mut camera_rotation_events: EventReader<NetworkData<messages::PlayerCameraRotation>>,
    mut camera_q: Query<&mut Transform, With<Camera>>,
) {
    for rotation_event in camera_rotation_events.iter() {
        let mut transform = camera_q.single_mut();
        transform.rotation = rotation_event.rotation;
    }
}

// TODO: Left unfinished, doesn't render outline.
// Target the block the player is looking at.
//fn outline_selected_block(
//    world_map: Res<WorldMap>,
//    camera_query: Query<&GlobalTransform, (With<Camera>, Changed<GlobalTransform>)>,
//) {
//    let camera_transform = camera_query.single();
//
//    // We need to find the first block the ray intersects with, it is then marked as the origin.
//    // From this point we can jump from one block to another easily.
//    let forward = camera_transform.forward();
//    let direction = forward.signum();
//
//    // How far along the forward vector you need to go to hit the next block in each direction.
//    // This makes more sense if you mentally align it with the block grid.
//    //
//    // Also this relies on some peculiar behaviour where normally f32.fract() would retain the sign
//    // of the fraction, vec3.fract() instead does self - self.floor(). This results in having the
//    // correct value for the negative direction, but it has to be flipped for the positive
//    // direction, which is the vec3::select.
//    let mut distance_next = camera_transform.translation.fract();
//    distance_next = Vec3::select(
//        direction.cmpeq(Vec3::ONE),
//        1.0 - distance_next,
//        distance_next,
//    );
//    distance_next = distance_next / forward.abs();
//
//    // How far along the forward vector you need to go to traverse one block in each direction.
//    let t_block = 1.0 / forward.abs();
//    // +/-1 to shift block_pos when it hits the grid
//    let step = direction.as_ivec3();
//
//    let mut block_pos = camera_transform.translation.floor().as_ivec3();
//
//    for _ in 0..5 {
//        if distance_next.x < distance_next.y && distance_next.x < distance_next.z {
//            block_pos.x += step.x;
//            distance_next.x += t_block.x;
//
//            if let Some(block_id) = world_map.get_block(&block_pos) {
//                if block_id == 0 {
//                    continue;
//                }
//                looked_at.0 = Some((
//                    block_pos,
//                    if direction.x == 1.0 {
//                        BlockSide::Left
//                    } else {
//                        BlockSide::Right
//                    },
//                ));
//                return;
//            }
//        } else if distance_next.z < distance_next.x && distance_next.z < distance_next.y {
//            block_pos.z += step.z;
//            distance_next.z += t_block.z;
//
//            if let Some(block_id) = world_map.get_block(&block_pos) {
//                if block_id == 0 {
//                    continue;
//                }
//                looked_at.0 = Some((
//                    block_pos,
//                    if direction.z == 1.0 {
//                        BlockSide::Back
//                    } else {
//                        BlockSide::Front
//                    },
//                ));
//                return;
//            }
//        } else {
//            block_pos.y += step.y;
//            distance_next.y += t_block.y;
//
//            if let Some(block_id) = world_map.get_block(&block_pos) {
//                if block_id == 0 {
//                    continue;
//                }
//                looked_at.0 = Some((
//                    block_pos,
//                    if direction.y == 1.0 {
//                        BlockSide::Bottom
//                    } else {
//                        BlockSide::Top
//                    },
//                ));
//                return;
//            }
//        }
//    }
//
//    looked_at.0 = None;
//}
