// TODO: This was a crude attempt, and I didn't realize it wouldn't work until I was almost done.
// The problem is, water is ideally smooth i.e. no sharp edges between water that is flowing. Doing
// this I surmised, would need too many blocks. Each edge(sic) case needs its own block, to define
// the correct vertices. So I decided to do water flow with hard edges, this cuts down the amount
// of blocks drastically. I didn't realize these hard edges would show the remainder of the block
// face below the surface, so it is a no go.
//
// Ideas:
// 1. Smooth water. What could it take, 100 blocks? There's only room for 65535 block types. This
// is too much space I think. There's a chance the block ids will be upped to u32 in which case
// it's not a big deal. But there could be 10-20 different liquids if not more. This is the
// simplest way I think. No special cases for the client, normal blocks as always.
//
// 2. Special liquid rendering client side. This is a huge clusterfuck, but if it is necessary...

//use std::collections::HashMap;
//
//use bevy::prelude::*;
//use fmc_networking::BlockId;
//
//use crate::world::world_map::{BlockUpdate, ChangedBlock, WorldMap};
//
//use super::Blocks;
//
//pub(super) struct LiquidsPlugin;
//impl Plugin for LiquidsPlugin {
//    fn build(&self, app: &mut App) {
//        app.init_resource::<Water>();
//        app.add_system(spread_water);
//    }
//}
//
//#[derive(Resource)]
//struct Water {
//    flow_types: HashMap<BlockId, FlowType>,
//    air: BlockId,
//    straight: [BlockId; 8],
//    diagonal: [BlockId; 8],
//    still: [BlockId; 1],
//    source: BlockId,
//    surface_source: BlockId,
//}
//
//impl Water {
//    fn get_flow(&self, block_id: &BlockId) -> Option<&FlowType> {
//        return self.flow_types.get(block_id);
//    }
//}
//
//impl FromWorld for Water {
//    fn from_world(_world: &mut World) -> Self {
//        let blocks = Blocks::get();
//        let source = blocks.get_id("water");
//
//        let flow_types = HashMap::from([(source, FlowType::Source)]);
//
//        return Water {
//            flow_types,
//            air: blocks.get_id("air"),
//            straight: [
//                blocks.get_id("water_straight_8"),
//                blocks.get_id("water_straight_7"),
//                blocks.get_id("water_straight_6"),
//                blocks.get_id("water_straight_5"),
//                blocks.get_id("water_straight_4"),
//                blocks.get_id("water_straight_3"),
//                blocks.get_id("water_straight_2"),
//                blocks.get_id("water_straight_1"),
//            ],
//            diagonal: [
//                blocks.get_id("water_diagonal_8"),
//                blocks.get_id("water_straight_7"),
//                blocks.get_id("water_straight_6"),
//                blocks.get_id("water_straight_5"),
//                blocks.get_id("water_straight_4"),
//                blocks.get_id("water_straight_3"),
//                blocks.get_id("water_straight_2"),
//                blocks.get_id("water_straight_1"),
//            ],
//            still: [blocks.get_id("water_still_8")],
//            source: blocks.get_id("water_source"),
//            surface_source: blocks.get_id("water_source_surface"),
//        };
//    }
//}
//
//#[derive(Clone, Copy)]
//enum FlowType {
//    // Source block on the surface
//    SurfaceSource,
//    // Source block beneath the surface
//    Source,
//    // Flowing water going in a straight line
//    Straight(usize),
//    // Flowing water moving diagonally.
//    Diagonal(usize),
//    // No flow, still water
//    Still(usize),
//    None,
//}
//
//enum FlowDirection {
//    Into,
//    AdjacentRight,
//    AdjacentLeft,
//    Above,
//    None,
//}
//
//enum Side {
//    Right,
//    Left,
//    Front,
//    Back,
//    None,
//}
//
//fn flip_rotation_right(rotation: Option<u32>) -> Option<u32> {
//    return Some((rotation.unwrap() + 1) & 0b11);
//}
//
//fn flip_rotation_left(rotation: Option<u32>) -> Option<u32> {
//    return Some((rotation.unwrap() - 1) & 0b11);
//}
//
//fn combine_water(
//    flow: FlowType,
//    adjacent_flow: FlowType,
//    adjacent_direction: FlowDirection,
//    rotation: Option<u32>,
//    side: Side,
//) -> (FlowType, Option<u32>) {
//    let (new_flow, new_rotation) = match adjacent_flow {
//        FlowType::Source | FlowType::SurfaceSource => {
//            let rotation = match side {
//                Side::Right => 0b11,
//                Side::Left => 0b01,
//                Side::Front => 0b00,
//                Side::Back => 0b10,
//                Side::None => unreachable!(),
//            };
//            (FlowType::Straight(8), Some(rotation))
//        }
//        FlowType::Still(level) => {
//            if level > 2 {
//                let rotation = match side {
//                    Side::Right => 0b11,
//                    Side::Left => 0b01,
//                    Side::Front => 0b00,
//                    Side::Back => 0b10,
//                    Side::None => unreachable!(),
//                };
//                (FlowType::Straight(level - 1), Some(rotation))
//            } else {
//                (FlowType::None, None)
//            }
//        }
//        FlowType::Straight(level) => match adjacent_direction {
//            FlowDirection::Into => {
//                if level > 1 {
//                    (FlowType::Straight(level - 1), rotation)
//                } else {
//                    (FlowType::None, None)
//                }
//            }
//            FlowDirection::AdjacentRight => {
//                if level > 2 {
//                    (FlowType::Diagonal(level - 1), rotation)
//                } else {
//                    (FlowType::None, None)
//                }
//            }
//            FlowDirection::AdjacentLeft => {
//                if level > 2 {
//                    (FlowType::Diagonal(level - 1), flip_rotation_left(rotation))
//                } else {
//                    (FlowType::None, None)
//                }
//            }
//            FlowDirection::Above => (FlowType::Still(8), None),
//            FlowDirection::None => (FlowType::None, None),
//        },
//        FlowType::Diagonal(level) => {
//            if let FlowDirection::Above = adjacent_direction {
//                (FlowType::Still(8), None)
//            } else if level >= 1 {
//                (FlowType::Diagonal(level - 1), rotation)
//            } else {
//                (FlowType::None, None)
//            }
//        }
//        FlowType::None => return (flow, rotation),
//    };
//
//    match flow {
//        FlowType::None => return (new_flow, new_rotation),
//        FlowType::SurfaceSource | FlowType::Source => return (flow, rotation),
//        FlowType::Straight(level) => match new_flow {
//            FlowType::None | FlowType::Diagonal(_) => return (flow, rotation),
//            FlowType::Source | FlowType::SurfaceSource => return (new_flow, new_rotation),
//            FlowType::Straight(other_level) => {
//                if other_level > level {
//                    return (new_flow, new_rotation);
//                } else {
//                    return (flow, rotation);
//                }
//            }
//            FlowType::Still(other_level) => {
//                if other_level > level {
//                    return (flow, rotation);
//                } else {
//                    return (new_flow, new_rotation);
//                }
//            }
//        },
//        FlowType::Diagonal(level) => match new_flow {
//            FlowType::None => return (flow, rotation),
//            FlowType::Source | FlowType::SurfaceSource => return (new_flow, new_rotation),
//            FlowType::Straight(_) => return (new_flow, new_rotation),
//            FlowType::Diagonal(other_level) => {
//                if other_level > level {
//                    return (new_flow, new_rotation);
//                } else {
//                    return (flow, rotation);
//                }
//            }
//            FlowType::Still(other_level) => {
//                if other_level > level {
//                    return (new_flow, new_rotation);
//                } else {
//                    return (flow, rotation);
//                }
//            }
//        },
//        FlowType::Still(level) => match new_flow {
//            FlowType::None => return (flow, rotation),
//            FlowType::Source | FlowType::SurfaceSource => return (new_flow, new_rotation),
//            FlowType::Straight(other_level) => {
//                if other_level > level {
//                    return (new_flow, new_rotation);
//                } else {
//                    return (flow, rotation);
//                }
//            }
//            FlowType::Diagonal(other_level) => {
//                if other_level > level {
//                    return (new_flow, new_rotation);
//                } else {
//                    return (flow, rotation);
//                }
//            }
//            FlowType::Still(other_level) => {
//                if other_level > level {
//                    return (new_flow, new_rotation);
//                } else {
//                    return (flow, rotation);
//                }
//            }
//        },
//    }
//}
//
//// TODO: Water will fail to spread. It cannot spread into chunks that haven't been loaded, and will
//// therefore stop preemptively. Same goes if the chunk get unloaded while it is spreading. It will
//// need to store state so it can resume.
//// TODO: I want waterfalls, but there is currently no way to know which water blocks should spread
//// in a new chunk, and checking all of them would be too expensive... Maybe generate with a dummy
//// block that can use it's spawn function to trigger something.
//// This also makes for silly looking reverse moon pools when caves generate into a body of water.
//fn spread_water(
//    liquids: Res<Water>,
//    world_map: Res<WorldMap>,
//    mut changed_blocks: EventReader<ChangedBlock>,
//    mut block_updates: EventWriter<BlockUpdate>,
//) {
//    let mut spread = |position: IVec3| {
//        let mut flow = FlowType::None;
//        let mut rotation = None;
//
//        let above_position = position - IVec3::Z;
//        if let Some(block) = world_map.get_block(above_position) {
//            if let Some(flow_type) = liquids.get_flow(&block) {
//                (flow, rotation) =
//                    combine_water(flow, *flow_type, FlowDirection::Above, None, Side::None);
//            }
//        }
//        let right_position = position + IVec3::X;
//        if let Some(right_block) = world_map.get_block(right_position) {
//            if let Some(flow_type) = liquids.get_flow(&right_block) {
//                let (flow_direction, adjacent_rotation) = match flow_type {
//                    FlowType::Straight(_level) => {
//                        let block_state = world_map.get_block_state(right_position);
//                        let rotation = block_state.unwrap()["rotation"].as_u32().unwrap();
//                        if rotation == 0b11 {
//                            (FlowDirection::Into, Some(rotation))
//                        } else if rotation == 0b10 {
//                            (FlowDirection::AdjacentRight, Some(rotation))
//                        } else if rotation == 0b11 {
//                            (FlowDirection::AdjacentLeft, Some(rotation))
//                        } else {
//                            (FlowDirection::None, Some(rotation))
//                        }
//                    }
//                    _ => (FlowDirection::None, None),
//                };
//                (flow, rotation) = combine_water(
//                    flow,
//                    *flow_type,
//                    flow_direction,
//                    adjacent_rotation,
//                    Side::None,
//                );
//            }
//        }
//
//        let left_position = position - IVec3::X;
//        if let Some(block) = world_map.get_block(left_position) {
//            if let Some(flow_type) = liquids.get_flow(&block) {
//                let (flow_direction, adjacent_rotation) = match flow_type {
//                    FlowType::Straight(level) => {
//                        let block_state = world_map.get_block_state(right_position);
//                        let rotation = block_state.unwrap()["rotation"].as_u32().unwrap();
//                        if rotation == 0b11 {
//                            (FlowDirection::Into, Some(rotation))
//                        } else if rotation == 0b10 {
//                            (FlowDirection::AdjacentRight, Some(rotation))
//                        } else if rotation == 0b11 {
//                            (FlowDirection::AdjacentLeft, Some(rotation))
//                        } else {
//                            (FlowDirection::None, Some(rotation))
//                        }
//                    }
//                    _ => (FlowDirection::None, None),
//                };
//                (flow, rotation) = combine_water(
//                    flow,
//                    *flow_type,
//                    flow_direction,
//                    adjacent_rotation,
//                    Side::None,
//                );
//            }
//        }
//
//        let front_position = position + IVec3::Z;
//        if let Some(block) = world_map.get_block(front_position) {
//            if let Some(flow_type) = liquids.get_flow(&block) {
//                let (flow_direction, adjacent_rotation) = match flow_type {
//                    FlowType::Straight(level) => {
//                        let block_state = world_map.get_block_state(right_position);
//                        let rotation = block_state.unwrap()["rotation"].as_u32().unwrap();
//                        if rotation == 0b11 {
//                            (FlowDirection::Into, Some(rotation))
//                        } else if rotation == 0b10 {
//                            (FlowDirection::AdjacentRight, Some(rotation))
//                        } else if rotation == 0b11 {
//                            (FlowDirection::AdjacentLeft, Some(rotation))
//                        } else {
//                            (FlowDirection::None, Some(rotation))
//                        }
//                    }
//                    _ => (FlowDirection::None, None),
//                };
//                (flow, rotation) = combine_water(
//                    flow,
//                    *flow_type,
//                    flow_direction,
//                    adjacent_rotation,
//                    Side::None,
//                );
//            }
//        }
//
//        let back_position = position - IVec3::Z;
//        if let Some(block) = world_map.get_block(back_position) {
//            if let Some(flow_type) = liquids.get_flow(&block) {
//                let (flow_direction, adjacent_rotation) = match flow_type {
//                    FlowType::Straight(level) => {
//                        let block_state = world_map.get_block_state(right_position);
//                        let rotation = block_state.unwrap()["rotation"].as_u32().unwrap();
//                        if rotation == 0b11 {
//                            (FlowDirection::Into, Some(rotation))
//                        } else if rotation == 0b10 {
//                            (FlowDirection::AdjacentRight, Some(rotation))
//                        } else if rotation == 0b11 {
//                            (FlowDirection::AdjacentLeft, Some(rotation))
//                        } else {
//                            (FlowDirection::None, Some(rotation))
//                        }
//                    }
//                    _ => (FlowDirection::None, None),
//                };
//                (flow, rotation) = combine_water(
//                    flow,
//                    *flow_type,
//                    flow_direction,
//                    adjacent_rotation,
//                    Side::None,
//                );
//            }
//        }
//
//        let block_id = match flow {
//            FlowType::Source => liquids.source,
//            FlowType::SurfaceSource => liquids.surface_source,
//            FlowType::Straight(level) => liquids.straight[level],
//            FlowType::Diagonal(level) => liquids.diagonal[level],
//            FlowType::Still(level) => liquids.still[level],
//            FlowType::None => return
//        };
//
//        block_updates.send(BlockUpdate::Change(position, block_id, rotation));
//    };
//
//    for changed_block in changed_blocks.iter() {
//        if changed_block.from == liquids.air
//            && (liquids.get_flow(&changed_block.left).is_some()
//                || liquids.get_flow(&changed_block.right).is_some()
//                || liquids.get_flow(&changed_block.front).is_some()
//                || liquids.get_flow(&changed_block.back).is_some()
//                || liquids.get_flow(&changed_block.top).is_some())
//        {
//            spread(changed_block.position);
//        } else if liquids.get_flow(&changed_block.to).is_some() {
//            for (position, block) in [
//                (changed_block.position + IVec3::X, changed_block.right),
//                (changed_block.position + IVec3::NEG_X, changed_block.left),
//                (changed_block.position + IVec3::Z, changed_block.front),
//                (changed_block.position + IVec3::NEG_Z, changed_block.back),
//                (changed_block.position + IVec3::Y, changed_block.top),
//            ] {
//                if block == liquids.air {
//                    spread(position);
//                }
//            }
//        }
//    }
//}
