use std::collections::HashMap;

use bevy::{
    math::{DVec3, Vec3A},
    prelude::*,
};
use fmc_networking::{messages, ConnectionId, NetworkData, NetworkServer};

use crate::{
    bevy_extensions::f64_transform::{F64GlobalTransform, F64Transform},
    physics::{shapes::Aabb, Velocity},
    players::{PlayerCamera, Players},
    utils,
    world::{
        blocks::{BlockFace, Blocks},
        items::{DroppedItem, Item, ItemStack, ItemStorage, Items},
        //blocks::Blocks,
        models::{Model, ModelBundle, ModelVisibility, Models},
        world_map::{chunk_manager::ChunkSubscriptions, BlockUpdate, WorldMap},
    },
};

use super::PlayerEquippedItem;

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
    player_query: Query<(&F64GlobalTransform, &PlayerCamera)>,
    mut model_query: Query<(&mut Model, &mut ModelVisibility), With<BreakingBlockTag>>,
    mut being_broken: Local<HashMap<IVec3, BreakingBlock>>,
) {
    let now = std::time::Instant::now();

    for click in clicks.iter() {
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
                Some(tuple) => tuple,
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
                    block_update_writer.send(BlockUpdate::Change(
                        block_pos,
                        blocks.get_id("air"),
                        None,
                    ));

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
                        // TODO: This velocity feels very off
                        Velocity(DVec3::new(velocity_x, 5.5, velocity_z)),
                        aabb,
                    ));
                }
            }
        } else {
            let model_entity = commands
                .spawn(ModelBundle {
                    model: Model::new(models.get_id("breaking_stage_1")),
                    // The model shouldn't show until some progress has been made
                    visibility: ModelVisibility::new(false),
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
    being_broken.drain_filter(|_, breaking_block| {
        let remove_timout = (now - breaking_block.prev_hit).as_secs_f32() > 0.5;
        let remove_broken = breaking_block.progress.as_secs_f32() >= 1.0;

        if remove_timout || remove_broken {
            commands.entity(breaking_block.model_entity).despawn();
            return true;
        }

        return false;
    });
}

// Process block events sent by the clients. Client should make sure that it is a valid placement.
pub fn place_blocks(
    net: Res<NetworkServer>,
    world_map: Res<WorldMap>,
    players: Res<Players>,
    items: Res<Items>,
    mut block_update_writer: EventWriter<BlockUpdate>,
    mut inventory_query: Query<(&mut ItemStorage, &PlayerEquippedItem), With<ConnectionId>>,
    mut block_update_events: EventReader<NetworkData<messages::BlockUpdates>>,
) {
    for event in block_update_events.iter() {
        let player_entity = players.get(&event.source);
        let (mut inventory, equipped_item) = inventory_query.get_mut(player_entity).unwrap();
        let equipped_item = &mut inventory[equipped_item.0];

        if equipped_item.is_empty() {
            // Illegal, don't try to place without something in your hand.
            net.disconnect(event.source);
            continue;
        }

        let item_config = items.get_config(&equipped_item.item().unwrap().id);

        let chunk = match world_map.get_chunk(&event.chunk_position) {
            Some(chunk) => chunk,
            None => {
                net.send_one(
                    event.source,
                    messages::Disconnect {
                        message: "Client sent block update for unloaded chunk.".to_owned(),
                    },
                );
                net.disconnect(event.source);
                continue;
            }
        };

        let blocks = Blocks::get();

        for (index, block_id) in event.blocks.iter() {
            if *block_id != item_config.block || equipped_item.size() == 0 {
                net.disconnect(event.source);
                continue;
            }

            equipped_item.subtract(1);

            if chunk[*index] == blocks.get_id("air") {
                block_update_writer.send(BlockUpdate::Change(
                    event.chunk_position + utils::block_index_to_position(*index),
                    *block_id,
                    event.block_state.get(index).copied(),
                ));
            }
        }
    }
}
