use std::collections::HashMap;

use bevy::{
    math::{DVec3, Vec3A},
    prelude::*,
};
use fmc_networking::{messages, ConnectionId, NetworkData, NetworkServer};

use crate::{
    bevy_extensions::f64_transform::{F64GlobalTransform, F64Transform},
    physics::{PhysicsBundle, Velocity},
    players::Players,
    world::{
        blocks::{BlockFace, BlockRotation, BlockState, Blocks, Friction},
        items::{DroppedItem, Item, ItemStack, ItemStorage, Items},
        //blocks::Blocks,
        models::{Model, ModelBundle, ModelVisibility, Models},
        world_map::{BlockUpdate, WorldMap},
    },
};

use super::player::{Camera, EquippedItem, Player};

// Keeps the state of how far along a block is to breaking
#[derive(Debug)]
pub struct BreakingBlock {
    pub model_entity: Entity,
    pub progress: std::time::Duration,
    pub prev_hit: std::time::Instant,
}

#[derive(Component)]
pub struct BreakingBlockTag;

// Left clicks are used for block breaking or attacking.
// TODO: Need spatial partitioning of item/mobs/players to do hit detection.
pub fn handle_left_clicks(
    mut commands: Commands,
    mut clicks: EventReader<NetworkData<messages::LeftClick>>,
    mut block_update_writer: EventWriter<BlockUpdate>,
    world_map: Res<WorldMap>,
    players: Res<Players>,
    items: Res<Items>,
    models: Res<Models>,
    player_query: Query<(&F64GlobalTransform, &Camera)>,
    mut model_query: Query<(&mut Model, &mut ModelVisibility), With<BreakingBlockTag>>,
    mut being_broken: Local<HashMap<IVec3, BreakingBlock>>,
) {
    let now = std::time::Instant::now();

    for click in clicks.read() {
        let player_entity = players.get(&click.source);
        let (player_position, player_camera) = player_query.get(player_entity).unwrap();

        // Raycast to the nearest block
        let camera_transform = F64Transform {
            translation: player_position.translation() + player_camera.translation,
            rotation: player_camera.rotation,
            ..default()
        };

        let (block_pos, block_id, _block_face) =
            match world_map.raycast_to_block(&camera_transform, 5.0) {
                Some(b) => b,
                None => continue,
            };

        if let Some(breaking_block) = being_broken.get_mut(&block_pos) {
            if now == breaking_block.prev_hit {
                // Block has already been hit this tick
                continue;
            } else if (now - breaking_block.prev_hit).as_secs_f32() > 0.05 {
                // The interval between two clicks needs to be short in order to be counted as
                // holding the button down.
                breaking_block.prev_hit = now;
                continue;
            } else {
                let (mut model, mut visibility) =
                    model_query.get_mut(breaking_block.model_entity).unwrap();

                let prev_progress = breaking_block.progress.as_secs_f32();

                breaking_block.progress += now - breaking_block.prev_hit;
                breaking_block.prev_hit = now;

                let progress = breaking_block.progress.as_secs_f32();

                if prev_progress < 0.1 && progress > 0.1 {
                    visibility.is_visible = true;
                } else if prev_progress < 0.2 && progress > 0.2 {
                    model.asset_id = models.get_id("breaking_stage_2");
                } else if prev_progress < 0.3 && progress > 0.3 {
                    model.asset_id = models.get_id("breaking_stage_3");
                } else if prev_progress < 0.4 && progress > 0.4 {
                    model.asset_id = models.get_id("breaking_stage_4");
                } else if prev_progress < 0.5 && progress > 0.5 {
                    model.asset_id = models.get_id("breaking_stage_5");
                } else if prev_progress < 0.6 && progress > 0.6 {
                    model.asset_id = models.get_id("breaking_stage_6");
                } else if prev_progress < 0.7 && progress > 0.7 {
                    model.asset_id = models.get_id("breaking_stage_7");
                } else if prev_progress < 0.8 && progress > 0.8 {
                    model.asset_id = models.get_id("breaking_stage_8");
                } else if prev_progress < 0.9 && progress > 0.9 {
                    model.asset_id = models.get_id("breaking_stage_9");
                } else if progress >= 1.0 {
                    let blocks = Blocks::get();
                    block_update_writer.send(BlockUpdate::Change {
                        position: block_pos,
                        block_id: blocks.get_id("air"),
                        block_state: None,
                    });

                    let block_config = blocks.get_config(&block_id);
                    let (dropped_item_id, count) = match block_config.drop() {
                        Some(drop) => drop,
                        None => continue,
                    };
                    let item_config = items.get_config(&dropped_item_id);
                    let model_config = models.get(&item_config.model_id);

                    let mut aabb = model_config.aabb.clone();

                    // We want to scale the model down to fit in a 0.15xYx0.15 box so the dropped
                    // item is fittingly small. Then extending the smallest horizontal dimension so
                    // that it becomes square.
                    const WIDTH: f64 = 0.075;
                    let max = aabb.half_extents.x.max(aabb.half_extents.z);
                    let scale = WIDTH / max;
                    aabb.half_extents.x = WIDTH;
                    aabb.half_extents.y *= scale;
                    aabb.half_extents.z = WIDTH;

                    let random = rand::random::<f64>() * std::f64::consts::TAU;
                    let (velocity_x, velocity_z) = random.sin_cos();

                    // For some reason the center has to be zeroed. Does bevy center gltf models?
                    // When the model is scaled does it shift the center(zeroing it like this would
                    // then be slightly off)?
                    aabb.center *= 0.0;
                    let translation =
                        block_pos.as_dvec3() + DVec3::splat(0.5) - DVec3::from(aabb.center);
                    //Offset the aabb slightly downwards to make the item float for clients.
                    aabb.center += DVec3::new(0.0, -0.1, 0.0);
                    commands.spawn((
                        DroppedItem(ItemStack::new(
                            Item::new(dropped_item_id),
                            count,
                            item_config.max_stack_size,
                        )),
                        ModelBundle {
                            model: Model::new(item_config.model_id),
                            visibility: ModelVisibility { is_visible: true },
                            global_transform: F64GlobalTransform::default(),
                            transform: F64Transform {
                                translation,
                                scale: DVec3::splat(scale),
                                ..default()
                            },
                        },
                        PhysicsBundle {
                            velocity: Velocity(DVec3::new(velocity_x, 5.5, velocity_z)),
                            ..default()
                        },
                        // TODO: This velocity feels off
                        aabb,
                    ));
                }
            }
        } else {
            let model_entity = commands
                .spawn(ModelBundle {
                    model: Model::new(models.get_id("breaking_stage_1")),
                    // The model shouldn't show until some progress has been made
                    visibility: ModelVisibility { is_visible: false },
                    global_transform: F64GlobalTransform::default(),
                    transform: F64Transform::from_translation(
                        block_pos.as_dvec3() + DVec3::splat(0.5),
                    ),
                })
                .insert(BreakingBlockTag)
                .id();

            // spawn new model
            being_broken.insert(
                block_pos,
                BreakingBlock {
                    model_entity,
                    progress: std::time::Duration::from_secs(0),
                    prev_hit: now,
                },
            );
        }
    }

    // Remove break progress after not being hit for 0.5 seconds.
    being_broken.retain(|_, breaking_block| {
        let remove_timout = (now - breaking_block.prev_hit).as_secs_f32() > 0.5;
        let remove_broken = breaking_block.progress.as_secs_f32() >= 1.0;

        if remove_timout || remove_broken {
            commands.entity(breaking_block.model_entity).despawn();
            return false;
        } else {
            return true;
        }
    });
}

// Process block events sent by the clients. Client should make sure that it is a valid placement.
pub fn handle_right_clicks(
    net: Res<NetworkServer>,
    world_map: Res<WorldMap>,
    players: Res<Players>,
    items: Res<Items>,
    mut clicks: EventReader<NetworkData<messages::RightClick>>,
    mut player_query: Query<
        (
            &mut ItemStorage,
            &EquippedItem,
            &F64GlobalTransform,
            &Camera,
        ),
        With<Player>,
    >,
    mut block_update_writer: EventWriter<BlockUpdate>,
) {
    for right_click in clicks.read() {
        let player_entity = players.get(&right_click.source);
        let (mut inventory, equipped_item, player_position, player_camera) =
            player_query.get_mut(player_entity).unwrap();

        let camera_transform = F64Transform {
            translation: player_position.translation() + player_camera.translation,
            rotation: player_camera.rotation,
            ..default()
        };

        let (block_pos, _, block_face) = match world_map.raycast_to_block(&camera_transform, 5.0) {
            Some(b) => b,
            None => continue,
        };

        let new_block_position = block_face.shift_position(block_pos);
        let block_id = world_map.get_block(new_block_position).unwrap();

        let blocks = Blocks::get();
        let block_config = blocks.get_config(&block_id);

        if !matches!(block_config.friction, Friction::Drag(_)) {
            dbg!(&block_config.name, block_face, new_block_position);
            continue;
        }

        let equipped_item = &mut inventory[equipped_item.0];

        if equipped_item.is_empty() {
            continue;
        }

        let item_config = items.get_config(&equipped_item.item().unwrap().id);
        equipped_item.subtract(1);

        // TODO: Placing blocks like stairs can be annoying, as situations often arise where your
        // position alone isn't adequate to find the correct placement.
        // There's a clever way to do this I think. If you partition a block face as such:
        //  -------------------
        //  | \_____________/ |
        //  | |             | |
        //  | |             | |
        //  | |             | |
        //  | |             | |
        //  | |_____________| |
        //  |/              \ |
        //  -------------------
        //  (Depicts 4 outer trapezoids and one inner square)
        // By comparing which sector was clicked and the angle of the camera I think a more
        // intuitive block placement can be achieved.
        let block_state = if blocks.get_config(&item_config.block).is_rotatable {
            let mut block_state = BlockState::default();

            if block_face == BlockFace::Bottom {
                let distance = player_position.translation().as_ivec3() - block_pos;
                let max = IVec2::new(distance.x, distance.z).max_element();

                if max == distance.x {
                    if distance.x.is_positive() {
                        block_state.set_rotation(BlockRotation::Once);
                        Some(block_state)
                    } else {
                        block_state.set_rotation(BlockRotation::Thrice);
                        Some(block_state)
                    }
                } else if max == distance.z {
                    if distance.z.is_positive() {
                        None
                    } else {
                        block_state.set_rotation(BlockRotation::Twice);
                        Some(block_state)
                    }
                } else {
                    unreachable!()
                }
            } else {
                block_state.set_rotation(block_face.to_rotation());
                Some(block_state)
            }
        } else {
            None
        };

        block_update_writer.send(BlockUpdate::Change {
            position: new_block_position,
            block_id: item_config.block,
            block_state,
        });
    }
}
