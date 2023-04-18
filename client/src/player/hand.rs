use std::collections::HashMap;

use bevy::{
    math::Vec3A, pbr::NotShadowCaster, prelude::*, render::primitives::Aabb, window::PrimaryWindow, animation::animation_player,
};
use fmc_networking::{messages, ConnectionId, NetworkClient, NetworkData};

use crate::{
    assets::models::Models,
    game_state::GameState,
    utils,
    world::{
        blocks::{Block, BlockFace, Blocks},
        world_map::WorldMap,
        Origin,
    },
};

use super::{
    camera::PlayerCameraMarker,
    interfaces::{
        items::{ItemId, ItemStack, Items},
        Interface, ItemBox, SelectedItemBox,
    },
    Player,
};

pub(super) const ANIMATION_LEN: f32 = 0.25;

pub struct HandPlugin;
impl Plugin for HandPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (equip_item, play_use_animation, place_block, send_clicks)
                .run_if(in_state(GameState::Playing)),
        );
    }
}

#[derive(Component)]
struct EquippedItem;


#[derive(Component)]
struct HandAnimationMarker;

// TODO: Animation can be defined at runtime through config files. Just have some parent file with
// the animations defined for block items and generic items. And then if you want to make something
// like a fishing rod, you define your own.
pub fn hand_setup(commands: &mut Commands) -> Entity {
    let mut animation_player = AnimationPlayer::default();
    animation_player.set_elapsed(ANIMATION_LEN);

    let entity = commands
        .spawn(SceneBundle::default())
        .insert(NotShadowCaster)
        .insert(Name::new("player_hand"))
        .insert(animation_player)
        .insert(HandAnimationMarker)
        .id();

    return entity;
}

// Equips the item that is selected in any visible interface where equipment=true in the config.
// There should only ever be one such interface visible, if there are more, it will equip one at
// random.
fn equip_item(
    mut commands: Commands,
    net: Res<NetworkClient>,
    items: Res<Items>,
    models: Res<Models>,
    changed_interface_query: Query<(&Interface, &SelectedItemBox), Changed<SelectedItemBox>>,
    item_box_query: Query<&ItemBox>,
    equipped_entity_query: Query<Entity, With<EquippedItem>>,
    changed_equipped_item_query: Query<&ItemStack, (Or<(Changed<ItemStack>, Added<EquippedItem>)>, With<EquippedItem>)>,
    mut animation_query: Query<(&mut AnimationPlayer, &mut Handle<Scene>), With<HandAnimationMarker>>,
) {
    // equip and unequip when the equipment interface is hidden/shown or the selected box changes
    for (interface, selected) in changed_interface_query.iter() {
        if !interface.config.is_equipment {
            continue;
        }

        if let Ok(entity) = equipped_entity_query.get_single() {
            commands.entity(entity).remove::<EquippedItem>();
        }

        let item_box = item_box_query.get(selected.0).unwrap();
        net.send_message(messages::InterfaceEquipItem {
            name: "hotbar".to_owned(),
            section: item_box.section_index as u32,
            index: item_box.index as u32,
        });
        
        commands.entity(selected.0).insert(EquippedItem);
    }

    // equip new item when the selected item changes.
    for item_stack in changed_equipped_item_query.iter() {
        let (mut animation_player, mut scene) = animation_query.single_mut();
        //visibility.is_visible {
        if let Some(item_id) = item_stack.item {
            let item = items.get(&item_id);
            let model = models.get(&item.model_id).unwrap();

            if &model.handle == scene.as_ref() {
                continue;
            }

            *scene = model.handle.clone();
            animation_player.start(item.equip_animation.clone());
            animation_player.set_elapsed(ANIMATION_LEN / 2.0);
        } else {
            *scene = Handle::default();
        }
        //} else {
        //    equipped_item.item = None;
        //    *scene = Handle::default();
        //}
    }
}

fn play_use_animation(
    window: Query<&Window, With<PrimaryWindow>>,
    mouse_button_input: Res<Input<MouseButton>>,
    mut equipped_query: Query<&mut AnimationPlayer, With<HandAnimationMarker>>,
) {
    if mouse_button_input.pressed(MouseButton::Left)
        || mouse_button_input.just_pressed(MouseButton::Right)
    {
        // Only play if not in an interface or settings menu.
        if window.single().cursor.visible {
            return;
        }

        let mut player = equipped_query.single_mut();
        if player.elapsed() >= ANIMATION_LEN {
            player.set_elapsed(0.0);
            player.resume();
        }
    }
}

// The server processes mouse clicks too.
fn send_clicks(mouse_button_input: Res<Input<MouseButton>>, net: Res<NetworkClient>) {
    if mouse_button_input.pressed(MouseButton::Left) {
        net.send_message(messages::LeftClick);
    } else if mouse_button_input.just_pressed(MouseButton::Right) {
        net.send_message(messages::RightClick);
    }
}

// TODO: Needs repetition if button held down. Test to where it feels reasonably comfortable so
// that you can fly and place without having to pace yourself.
//
// Place a block locally, the server will parse
fn place_block(
    net: Res<NetworkClient>,
    world_map: ResMut<WorldMap>,
    items: Res<Items>,
    origin: Res<Origin>,
    mouse_button_input: Res<Input<MouseButton>>,
    mut equipped_query: Query<&mut ItemStack, With<EquippedItem>>,
    player_query: Query<(&Aabb, &GlobalTransform), With<Player>>,
    camera_transform: Query<&GlobalTransform, With<PlayerCameraMarker>>,
    // We pretend the block update came from the server so it instantly updates without having to
    // rebound of the server.
    mut block_updates_events: EventWriter<NetworkData<messages::BlockUpdates>>,
) {
    if mouse_button_input.just_pressed(MouseButton::Right) {
        let (player_aabb, player_position) = player_query.single();
        let camera_transform = camera_transform.single();
        let mut equipped_item = equipped_query.single_mut();
        let blocks = Blocks::get();

        let (mut block_position, _block_id, block_face) = match world_map.raycast_to_block(
            &camera_transform.compute_transform(),
            origin.0,
            5.0,
        ) {
            Some(i) => i,
            None => return,
        };

        match block_face {
            BlockFace::Top => block_position.y += 1,
            BlockFace::Bottom => block_position.y -= 1,
            BlockFace::Front => block_position.z += 1,
            BlockFace::Back => block_position.z -= 1,
            BlockFace::Right => block_position.x += 1,
            BlockFace::Left => block_position.x -= 1,
        }

        let block_aabb = Aabb::from_min_max(
            (block_position - origin.0).as_vec3(),
            (block_position + 1 - origin.0).as_vec3(),
        );

        // TODO: This is too strict, you can't place blocks directly beneath / adjacently when
        // standing on an edge.
        let overlap = player_aabb.half_extents + block_aabb.half_extents
            - (player_aabb.center + player_position.translation_vec3a() - block_aabb.center).abs();

        if overlap.cmpgt(Vec3A::ZERO).all() {
            return;
        }

        let block_id = match equipped_item.item {
            Some(item_id) => match &items.get(&item_id).block {
                Some(block_id) => block_id,
                None => return,
            },
            None => return,
        };

        equipped_item.subtract(1);

        let block = &blocks[&block_id];

        let (chunk_position, block_index) =
            utils::world_position_to_chunk_position_and_block_index(block_position);
        let message = messages::BlockUpdates {
            chunk_position,
            blocks: vec![(block_index, *block_id)],
            block_state: HashMap::from([(block_index, 0)]),
        };

        net.send_message(message.clone());

        // Pretend we get the block from the server so it gets the update immediately for mesh
        // generation. More responsive.
        match block {
            Block::Cube(_) => {
                block_updates_events.send(NetworkData::new(ConnectionId::default(), message))
            }
            Block::Model(_) => {
                block_updates_events.send(NetworkData::new(ConnectionId::default(), message))
            }
        }
    }
}

//fn bobbing(
//    equipped_query: Query<&mut Transform, With<EquippedMarker>>
//) {
//
//}
