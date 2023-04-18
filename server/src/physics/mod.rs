pub mod shapes;

use bevy::{
    math::{BVec3A, DVec3, Vec3A},
    prelude::*,
};

use crate::{
    bevy_extensions::f64_transform::{F64GlobalTransform, F64Transform},
    world::{
        blocks::{Blocks, Friction},
        world_map::WorldMap,
    },
};

use self::shapes::Aabb;

pub struct PhysicsPlugin;
impl Plugin for PhysicsPlugin {
    fn build(&self, app: &mut App) {
        //app.add_system_to_stage(CoreStage::PostUpdate, simulate_aabb_physics);
        app.add_systems(Update, simulate_aabb_physics);
    }
}

/// Speed and direction of an object that should have physics.
#[derive(Component, Deref, DerefMut)]
pub struct Velocity(pub DVec3);

// TODO: Friction
// TODO: The entities will not move if they have come to a standstill, this is to refrain from
// having to compute physics for all entities every tick. I think blocks should instead trigger a
// mut deref of any entities in its vicinity. This can just be a separate system that listens for
// blockchange events.
// BUG: Wanted to use Vec3A e2e, but the Vec3A::max_element function considers NaN to be greater
// than any number, where Vec3::max_element is opposite.
//
// Moves all entities with an aabb along their velocity vector and resolves any collisions that
// occur with the terrain.
fn simulate_aabb_physics(
    world_map: Res<WorldMap>,
    time: Res<Time>,
    mut entities: Query<(&mut F64Transform, &mut Velocity, &Aabb)>,
) {
    for (mut transform, mut velocity, aabb) in entities.iter_mut() {
        // Have to check changes manually because change detection does not detect changes within
        // its own system.
        if !transform.is_changed() && velocity.0 == DVec3::ZERO {
            continue;
        }

        const GRAVITY: DVec3 = DVec3::new(0.0, -28.0, 0.0);
        velocity.0 += GRAVITY * time.delta_seconds_f64();

        // TODO: Maybe until Fixed time step run criteria it can do delta_seconds/ms_per_update
        // iterations
        let pos_after_move = transform.translation + velocity.0 * time.delta_seconds_f64();

        let aabb = Aabb {
            center: aabb.center + pos_after_move,
            half_extents: aabb.half_extents,
        };

        let blocks = Blocks::get();

        // Check for collisions for all blocks within the aabb.
        let mut collisions = Vec::new();
        let start = aabb.min().floor().as_ivec3();
        let stop = aabb.max().floor().as_ivec3();
        for x in start.x..=stop.x {
            for y in start.y..=stop.y {
                for z in start.z..=stop.z {
                    let block_pos = IVec3::new(x, y, z);
                    // TODO: This looks up chunk through hashmap each time, is too bad?
                    let block_id = match world_map.get_block(block_pos) {
                        Some(id) => id,
                        // If entity is player disconnect? They should always have their
                        // surroundings loaded.
                        None => continue,
                    };

                    // TODO: Take into account drag
                    match blocks.get_config(&block_id).friction {
                        Friction::Drag(_) => continue,
                        _ => (),
                    }

                    let block_aabb = Aabb {
                        center: block_pos.as_dvec3() + 0.5,
                        half_extents: DVec3::new(0.5, 0.5, 0.5),
                    };

                    let overlap = aabb.half_extents + block_aabb.half_extents
                        - (aabb.center - block_aabb.center).abs();

                    if overlap.cmpgt(DVec3::ZERO).all() {
                        collisions.push((overlap, block_id));
                    }
                }
            }
        }

        let mut move_back = DVec3::ZERO;
        let delta_time = DVec3::splat(time.delta_seconds_f64());

        // Resolve the conflicts by moving the aabb the opposite way of the velocity vector on the
        // axis it takes the longest time to resolve the conflict.
        for (collision, _block_id) in collisions {
            let backwards_time = collision / velocity.abs();
            // Small epsilon to delta time because of precision.
            let valid_axes = backwards_time.cmplt(delta_time + 0.0001);
            let slowest_resolution_axis =
                DVec3::select(valid_axes, backwards_time, DVec3::NAN).max_element();

            const EPSILON: f64 = f64::EPSILON * 10.0;
            if slowest_resolution_axis == backwards_time.x {
                move_back.x = backwards_time.x * -velocity.x + (-velocity.x).signum() * EPSILON;
                velocity.x = 0.0;
            } else if slowest_resolution_axis == backwards_time.y {
                move_back.y = backwards_time.y * -velocity.y + (-velocity.y).signum() * EPSILON;
                //println!("resolved y: {}, {}", move_back.y, velocity.y);
                if velocity.y.signum() == GRAVITY.y.signum() {
                    velocity.0 = DVec3::ZERO;
                } else {
                    velocity.y = 0.0;
                }
            } else if slowest_resolution_axis == backwards_time.z {
                move_back.z = backwards_time.z * -velocity.z + (-velocity.z).signum() * EPSILON;
                velocity.z = 0.0;
            }
        }

        transform.translation = pos_after_move + move_back;
    }
}
