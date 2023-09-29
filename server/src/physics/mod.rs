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

const GRAVITY: DVec3 = DVec3::new(0.0, -28.0, 0.0);

pub struct PhysicsPlugin;
impl Plugin for PhysicsPlugin {
    fn build(&self, app: &mut App) {
        //app.add_system_to_stage(CoreStage::PostUpdate, simulate_aabb_physics);
        app.add_systems(Update, simulate_aabb_physics);
    }
}

/// Enables physics simulation for an entity
#[derive(Component, Default)]
pub struct Mass;

/// Speed and direction of an object that should have physics.
#[derive(Component, Default, Deref, DerefMut)]
pub struct Velocity(pub DVec3);

#[derive(Bundle, Default)]
pub struct PhysicsBundle {
    pub mass: Mass,
    pub velocity: Velocity,
}

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
    mut entities: Query<(&mut F64Transform, &mut Velocity, &Aabb), With<Mass>>,
) {
    for (mut transform, mut velocity, aabb) in entities.iter_mut() {
        // Have to check changes manually because change detection does not detect changes within
        // its own system.
        if !transform.is_changed() && velocity.0 == DVec3::ZERO {
            continue;
        }

        velocity.0 += GRAVITY * time.delta_seconds_f64();

        for directional_velocity in [
            DVec3::new(0.0, velocity.y, 0.0),
            DVec3::new(velocity.x, 0.0, 0.0),
            DVec3::new(0.0, 0.0, velocity.z),
        ] {
            let pos_after_move =
                transform.translation + directional_velocity * time.delta_seconds_f64();

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

                        let distance = aabb.center - block_aabb.center;
                        let overlap = aabb.half_extents + block_aabb.half_extents
                            - distance.abs();

                        if overlap.cmpgt(DVec3::ZERO).all() {
                            //collisions.push((overlap, block_id));
                            collisions.push((DVec3::from(overlap.copysign(distance)), block_id));
                        }
                    }
                }
            }

            let mut move_back = DVec3::ZERO;
            let delta_time = DVec3::splat(time.delta_seconds_f64());

            // Resolve the conflicts by moving the aabb the opposite way of the velocity vector on the
            // axis it takes the longest time to resolve the conflict.
            for (collision, _block_id) in collisions {
                let backwards_time = collision / -directional_velocity;
                // Small epsilon to delta time because of precision.
                let valid_axes = backwards_time.cmplt(delta_time + delta_time / 100.0)
                    & backwards_time.cmpgt(DVec3::ZERO);
                let resolution_axis =
                    DVec3::select(valid_axes, backwards_time, DVec3::NAN).max_element();

                if resolution_axis == backwards_time.y {
                    move_back.y = collision.y + collision.y / 100.0;
                    if directional_velocity.y.signum() == GRAVITY.y.signum() {
                        velocity.0 = DVec3::ZERO;
                    } else {
                        velocity.y = 0.0;
                    }
                } else if resolution_axis == backwards_time.x {
                    move_back.x = collision.x + collision.x / 100.0;
                    velocity.x = 0.0;
                } else if resolution_axis == backwards_time.z {
                    move_back.z = collision.z + collision.z / 100.0;
                    velocity.z = 0.0;
                }
            }

            transform.translation = pos_after_move + move_back;
        }
    }
}
