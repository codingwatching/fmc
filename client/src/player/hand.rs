use std::{collections::HashMap, time::Duration};

use bevy::{
    gltf::{Gltf, GltfMesh},
    math::Vec3A,
    prelude::*,
    render::{mesh::VertexAttributeValues, primitives::Aabb},
    window::PrimaryWindow,
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
        items::{ItemStack, Items},
        Interface, ItemBox, SelectedItemBox,
    },
    Player,
};

pub struct HandPlugin;
impl Plugin for HandPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(SwitchAnimation::default()).add_systems(
            Update,
            (
                equip_item,
                play_use_animation,
                play_switch_animation,
                place_block,
                send_clicks,
            )
                .run_if(in_state(GameState::Playing)),
        );
    }
}

#[derive(Component)]
struct EquippedItem;

#[derive(Component)]
struct HandMarker;

// TODO: Animation can be defined at runtime through config files. Just have some parent file with
// the animations defined for block items and generic items. And then if you want to make something
// like a fishing rod, you define your own.
pub fn hand_setup(commands: &mut Commands) -> Entity {
    let entity = commands
        .spawn(SceneBundle::default())
        .insert(AnimationPlayer::default())
        .insert(HandMarker)
        .id();

    return entity;
}

#[derive(Resource, Default)]
struct SwitchAnimation {
    elapsed: f32,
    old_transform: Transform,
    old_offset: f32,
    new_transform: Transform,
    new_offset: f32,
    scene_handle: Handle<Scene>,
}
// Equips the item that is selected in any visible interface where equipment=true in the config.
// There should only ever be one such interface visible, if there are more, it will equip one at
// random.
fn equip_item(
    mut commands: Commands,
    net: Res<NetworkClient>,
    items: Res<Items>,
    models: Res<Models>,
    gltf_assets: Res<Assets<Gltf>>,
    gltf_meshes: Res<Assets<GltfMesh>>,
    meshes: Res<Assets<Mesh>>,
    animation_clips: Res<Assets<AnimationClip>>,
    mut switch_animation: ResMut<SwitchAnimation>,
    changed_interface_query: Query<(&Interface, &SelectedItemBox), Changed<SelectedItemBox>>,
    item_box_query: Query<&ItemBox>,
    equipped_entity_query: Query<Entity, With<EquippedItem>>,
    changed_equipped_item_query: Query<
        &ItemStack,
        (
            Or<(Changed<ItemStack>, Added<EquippedItem>)>,
            With<EquippedItem>,
        ),
    >,
    hand_scene_query: Query<&Handle<Scene>, With<HandMarker>>,
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
        let scene = hand_scene_query.single();

        switch_animation.old_transform = switch_animation.new_transform;
        switch_animation.old_offset = switch_animation.new_offset;

        let mut new_transform = Transform::default();

        if let Some(item_id) = item_stack.item {
            let item = items.get(&item_id);
            let model = models.get(&item.model_id).unwrap();
            let gltf = gltf_assets.get(&model.handle).unwrap();

            // This looks like it is only to prevent triggering the animation when switching
            // between the same items. The server sends a full interface update anytime an item is
            // picked up, that is also caught by this.
            if gltf.scenes[0] == *scene {
                continue;
            }

            let gltf_mesh = gltf_meshes.get(&gltf.meshes[0]).unwrap();
            // Cumbersomely extract aabb height from gltf in an error prone way. I don't know how
            // to do it through the scenes.
            let mut min: f32 = 0.0;
            let mut max: f32 = 0.0;
            for primitive in gltf_mesh.primitives.iter() {
                let mesh = meshes.get(&primitive.mesh).unwrap();
                let Some(VertexAttributeValues::Float32x3(vertices)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else {continue};
                for vertex in vertices.iter() {
                    min = min.min(vertex[1]);
                    max = max.max(vertex[1]);
                }
            }
            let height = max - min;

            let animation_handle = gltf.named_animations.get("left_click").unwrap().clone();
            let animation_clip = animation_clips.get(&animation_handle).unwrap();

            for curve in &animation_clip.curves()[0] {
                match &curve.keyframes {
                    Keyframes::Scale(frames) => {
                        new_transform.scale = *frames.last().unwrap();
                    }
                    Keyframes::Translation(frames) => {
                        new_transform.translation = *frames.last().unwrap();
                    }
                    Keyframes::Rotation(frames) => {
                        new_transform.rotation = *frames.last().unwrap();
                    }
                }
            }

            switch_animation.new_transform = new_transform;
            switch_animation.new_offset = height;
            switch_animation.scene_handle = gltf.scenes[0].clone();
            switch_animation.elapsed = 0.0;
        } else {
            switch_animation.scene_handle = Handle::default();
            switch_animation.elapsed = 0.0;
        }
    }
}

fn play_switch_animation(
    time: Res<Time>,
    mut switch_animation: ResMut<SwitchAnimation>,
    mut hand_query: Query<(&mut Transform, &mut Handle<Scene>), With<HandMarker>>,
) {
    const DURATION: f32 = 0.3;

    let (mut transform, mut scene) = hand_query.single_mut();

    if switch_animation.elapsed < DURATION / 2.0 {
        let mut new_transform = switch_animation.old_transform;
        new_transform.translation.y -= switch_animation.elapsed * switch_animation.old_offset;
        *transform = new_transform;

        if switch_animation.elapsed + time.delta_seconds() > DURATION / 2.0 {
            *scene = switch_animation.scene_handle.clone();
        }
    } else if switch_animation.elapsed < DURATION {
        let mut new_transform = switch_animation.new_transform;
        new_transform.translation.y -=
            (DURATION - switch_animation.elapsed) * switch_animation.new_offset;
        *transform = new_transform;
    }
    switch_animation.elapsed += time.delta_seconds();
}

fn play_use_animation(
    items: Res<Items>,
    models: Res<Models>,
    gltf_assets: Res<Assets<Gltf>>,
    animation_clips: Res<Assets<AnimationClip>>,
    window: Query<&Window, With<PrimaryWindow>>,
    mouse_button_input: Res<Input<MouseButton>>,
    mut hand_animation_query: Query<&mut AnimationPlayer, With<HandMarker>>,
    equipped_item_query: Query<&ItemStack, With<EquippedItem>>,
) {
    let Ok(equipped_item) = equipped_item_query.get_single() else {return};

    // Only play if not in an interface or settings menu.
    if window.single().cursor.visible {
        return;
    }

    let item = if let Some(item_id) = &equipped_item.item {
        items.get(item_id)
    } else {
        return;
    };

    let model = models.get(&item.model_id).unwrap();
    let gltf = gltf_assets.get(&model.handle).unwrap();
    let mut player = hand_animation_query.single_mut();

    if mouse_button_input.pressed(MouseButton::Left) {
        let animation_handle = gltf.named_animations.get("left_click").unwrap();
        let animation_clip = animation_clips.get(&animation_handle).unwrap();
        if mouse_button_input.just_pressed(MouseButton::Left) {
            player.start_with_transition(animation_handle.clone(), Duration::from_millis(10));
        } else if player.elapsed() >= animation_clip.duration() {
            player.start(animation_handle.clone());
        }
    } else if mouse_button_input.just_pressed(MouseButton::Right) {
        player.start_with_transition(
            gltf.named_animations.get("left_click").unwrap().clone(),
            Duration::from_millis(10),
        );
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
