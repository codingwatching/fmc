// TODO:
use std::{
    collections::{HashMap, HashSet},
    io::Read,
};

use bevy::{
    prelude::*,
    render::texture::CompressedImageFormats,
    window::{CursorGrabMode, PrimaryWindow},
};

use fmc_networking::{messages, NetworkClient, NetworkData};
use serde::{Deserialize, Serialize};

use crate::game_state::GameState;

use super::items::{ItemStack, Items};

const INTERFACE_CONFIG_PATH: &str = "server_assets/interfaces/";

// TODO: I decided to use "take/place" instead of "swap" to move items around interfaces. This was
// to limit the frustration of the players if several were to use the same interface at once. This
// would lead to "stealing" items from each other when several hold the same item. I'm beginning to
// think it was a bad idea. It would have simpler code in exchange for some slightly weird
// behaviour. Do a think about this.

// TODO: The item grid in the inventory can only have 7 columns, if more the layout breaks.

// TODO: Makes no sense to have this under crate::player::interfaces? This only covers in-game
// interfaces that are defined by the server at runtime, separate from the hardcoded ui used for
// the main menu and settings. Can't come up with a good name to separate the two.
pub struct InterfacePlugin;
impl Plugin for InterfacePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(UiScale { scale: 4.0 })
            .insert_resource(InterfaceStack::default())
            .add_event::<InterfaceToggleEvent>()
            .add_systems(
                Update,
                (
                    handle_interface_item_box_updates,
                    handle_interface_toggle_events,
                    handle_interface_open_request,
                    handle_interface_close_request,
                    item_box_mouse_interaction,
                    update_cursor_item_stack_position,
                    update_item_box_images,
                    cursor_visibility,
                    initial_select_item_box,
                    keyboard_select_item_box,
                )
                    .run_if(in_state(GameState::Playing)),
            );
    }
}

/// Event used by keybindings to toggle an interface open or closed.
pub struct InterfaceToggleEvent(pub Entity);

#[derive(Component)]
struct ItemBoxSectionMarker;

#[derive(Component)]
pub struct ItemBox {
    // Interface it belongs to
    pub interface_entity: Entity,
    // Section index in the interface config
    pub section_index: usize,
    // Box index in the section
    pub index: usize,
}

#[derive(Resource, Deref, DerefMut, Default)]
pub struct Interfaces(HashMap<String, Entity>);

// Interfaces that are open and allow other interfaces to take focus are stored here while the
// focused one is visible. This is temporary as only one interface can be open at a time right now.
// TODO: This is a constraint of bevy having one common root UI node. EDIT: Looking at this later
// seems to me can just switch from using Visibility::Hidden to Display::None
#[derive(Resource, Deref, DerefMut, Default)]
struct InterfaceStack(Vec<Entity>);

#[derive(Component)]
pub struct Interface {
    pub config: InterfaceConfig,
    // Sections item boxes can use as parents.
    item_box_section_entities: Vec<Entity>,
}

pub struct InterfaceConfig {
    /// Interface name
    pub name: String,
    /// If it should overlap(non exclusive) and replace(exclusive) interfaces when opened.
    pub is_exclusive: bool,
    // TODO: This is too bespoke?, see if there isn't a more general way to do it.
    /// Equip item when it is selected.
    pub is_equipment: bool,
    /// Sections where item boxes can be put.
    item_box_sections: Vec<ItemBoxSectionConfig>,
    /// The image used as the interface background.
    image_path: String,
    /// The root position of the interface in percent. [Left, Top]
    position: UiRect,
    /// Margin used to position the interface if there is no position given for an axis.
    margin: UiRect,
}

#[derive(Serialize, Deserialize)]
struct InterfaceConfigJson {
    name: String,
    exclusive: Option<bool>,
    equipment: Option<bool>,
    item_box_sections: Vec<ItemBoxSectionConfigJson>,
    image: String,
    position: Option<Rect>,
}

impl From<InterfaceConfigJson> for InterfaceConfig {
    fn from(value: InterfaceConfigJson) -> Self {
        let position = match value.position {
            Some(position) => UiRect {
                left: match position.left {
                    Some(l) => Val::Percent(l),
                    None => Val::Auto,
                },
                right: match position.right {
                    Some(r) => Val::Percent(r),
                    None => Val::Auto,
                },
                top: match position.top {
                    Some(t) => Val::Percent(t),
                    None => Val::Auto,
                },
                bottom: match position.bottom {
                    Some(b) => Val::Percent(b),
                    None => Val::Auto,
                },
            },
            None => UiRect::all(Val::Auto),
        };

        // TODO: Idk if all this is necessary it was just leftover from some previous code. And
        // this ui stuff is a mess to make display correctly.
        let left = if let Val::Percent(left) = position.left {
            Val::Vw(left)
        } else {
            Val::Auto
        };
        let right = if let Val::Percent(right) = position.right {
            Val::Vw(right)
        } else {
            Val::Auto
        };
        let top = if let Val::Percent(top) = position.top {
            Val::Vh(top)
        } else {
            Val::Auto
        };
        let bottom = if let Val::Percent(bottom) = position.bottom {
            Val::Vh(bottom)
        } else {
            Val::Auto
        };

        let margin = UiRect {
            left,
            right,
            top,
            bottom,
        };

        Self {
            name: value.name,
            is_exclusive: value.exclusive.unwrap_or(false),
            is_equipment: value.equipment.unwrap_or(false),
            item_box_sections: value
                .item_box_sections
                .into_iter()
                .map(ItemBoxSectionConfig::from)
                .collect(),
            image_path: "server_assets/textures/interfaces/".to_owned() + &value.image,
            position,
            margin,
        }
    }
}

#[derive(Serialize, Deserialize)]
struct Rect {
    left: Option<f32>,
    right: Option<f32>,
    top: Option<f32>,
    bottom: Option<f32>,
}

#[derive(Serialize, Deserialize)]
struct ItemBoxSectionConfig {
    /// If it is allowed to quick move to this section.
    allow_quick_place: bool,
    /// Which item types can be placed in this section.
    allowed_item_types: Option<HashSet<String>>,
    /// Position of section relative to top left corner of the ui image in pixels.
    position: [f32; 2],
    /// Size of the section in pixels. Needs to be a multiple of 15 (the size of an item box)
    size: [f32; 2],
}

#[derive(Serialize, Deserialize)]
struct ItemBoxSectionConfigJson {
    allow_quick_place: Option<bool>,
    allowed_item_types: Option<HashSet<String>>,
    position: [f32; 2],
    size: [f32; 2],
}

impl From<ItemBoxSectionConfigJson> for ItemBoxSectionConfig {
    fn from(value: ItemBoxSectionConfigJson) -> Self {
        ItemBoxSectionConfig {
            allow_quick_place: value.allow_quick_place.unwrap_or(true),
            allowed_item_types: value.allowed_item_types,
            position: value.position,
            size: value.size,
        }
    }
}

// The item stack is unique, and shared between all interfaces(since only one can be open at a
// time). When the interface is closed, the item is returned to the interface it was taken from.
//
/// Marker for the item stack that is held by the mouse cursor when an interface is open.
#[derive(Component)]
struct CursorItemStackMarker;

// Called when loading assets.
pub fn load_interfaces(
    mut commands: Commands,
    net: Res<NetworkClient>,
    asset_server: Res<AssetServer>,
) {
    let mut interfaces = Interfaces::default();
    let directory = match std::fs::read_dir(INTERFACE_CONFIG_PATH) {
        Ok(dir) => dir,
        Err(e) => {
            net.disconnect(&format!(
                "Misconfigured resource pack: Failed to read interface configuration directory '{}'\n\
                Error: {}",
                INTERFACE_CONFIG_PATH, e
            ));
            return;
        }
    };

    for dir_entry in directory {
        let file_path = match dir_entry {
            Ok(d) => d.path(),
            Err(e) => {
                net.disconnect(&format!(
                    "Misconfigured resource pack: Failed to read the file path of an interface config\n\
                    Error: {}",
                    e
                ));
                return;
            }
        };
        let file = match std::fs::File::open(&file_path) {
            Ok(f) => f,
            Err(e) => {
                net.disconnect(&format!(
                    "Misconfigured resource pack: Failed to read interface configuration at: '{}'\n\
                    Error: {}",
                    &file_path.display(),
                    e
                ));
                return;
            }
        };
        let interface_config_json: InterfaceConfigJson = match serde_json::from_reader(&file) {
            Ok(c) => c,
            Err(e) => {
                net.disconnect(&format!(
                    "Misconfigured resource pack: Failed to read interface configuration at: '{}'\n\
                    Error: {}",
                    &file_path.display(),
                    e
                ));
                return;
            }
        };

        let interface_config = InterfaceConfig::from(interface_config_json);
        let mut interface = Interface {
            config: interface_config,
            item_box_section_entities: Vec::new(),
        };

        // TODO: We have to read the size of the image manually, it would be great if bevy could
        // just do this automatically when Size has Val::Undefined (default). Written at bevy
        // version  0.8.
        let mut file = match std::fs::File::open(&interface.config.image_path) {
            Ok(f) => f,
            Err(e) => {
                net.disconnect(format!(
                    "Misconfigured resource pack: No interface image found at {}\nError: {}",
                    &interface.config.image_path, e
                ));
                return;
            }
        };

        let mut image_data = Vec::new();
        match file.read_to_end(&mut image_data) {
            Ok(..) => (),
            Err(e) => {
                net.disconnect(format!(
                    "Misconfigured resource pack: Failed to read interface image found at {}\nError: {}",
                    &interface.config.image_path, e
                ));
                return;
            }
        }

        let image = match Image::from_buffer(
            &image_data,
            bevy::render::texture::ImageType::Extension("png"),
            CompressedImageFormats::NONE,
            false,
        ) {
            Ok(i) => i,
            Err(e) => {
                net.disconnect(format!(
                    "Misconfigured resource pack: Failed to read image data found at {}\nError: {}",
                    &interface.config.image_path, e
                ));
                return;
            }
        };

        let entity = commands
            // Root node
            .spawn(NodeBundle {
                style: Style {
                    size: Size {
                        width: Val::Percent(100.0),
                        height: Val::Percent(100.0),
                    },
                    position_type: PositionType::Absolute,
                    ..default()
                },
                visibility: Visibility::Hidden,
                ..default()
            })
            .with_children(|parent| {
                // ui image
                parent
                    .spawn(ImageBundle {
                        style: Style {
                            size: Size {
                                width: Val::Px(image.texture_descriptor.size.width as f32),
                                height: Val::Px(image.texture_descriptor.size.height as f32),
                            },
                            left: interface.config.position.left,
                            right: interface.config.position.right,
                            top: interface.config.position.top,
                            bottom: interface.config.position.bottom,
                            // TODO: This is not supposed to be here, added to make hotbar go to
                            // bottom.
                            align_self: AlignSelf::FlexEnd,
                            margin: interface.config.margin,
                            ..default()
                        },
                        image: asset_server.load(&interface.config.image_path).into(),
                        ..default()
                    })
                    // item box sections
                    .with_children(|parent| {
                        for section_config in interface.config.item_box_sections.iter() {
                            let entity = parent
                                .spawn(NodeBundle {
                                    style: Style {
                                        position_type: PositionType::Absolute,
                                        left: Val::Px(section_config.position[0]),
                                        top: Val::Px(section_config.position[1]),
                                        size: Size {
                                            width: Val::Px(section_config.size[0]),
                                            height: Val::Px(section_config.size[1]),
                                        },
                                        flex_wrap: FlexWrap::WrapReverse,
                                        justify_content: JustifyContent::SpaceBetween,
                                        align_content: AlignContent::SpaceBetween,
                                        ..default()
                                    },
                                    ..default()
                                })
                                .insert(ItemBoxSectionMarker)
                                .id();
                            interface.item_box_section_entities.push(entity);
                        }
                    });
            })
            .insert(interface)
            .id();

        let name = file_path.file_stem().unwrap().to_str().unwrap().to_owned();
        interfaces.insert(name, entity);
    }

    commands.insert_resource(interfaces);

    // Cursor item stack / held item
    commands
        .spawn(ImageBundle {
            style: Style {
                size: Size::new(Val::Px(16.0), Val::Px(16.0)),
                position_type: PositionType::Absolute,
                flex_direction: FlexDirection::ColumnReverse,
                align_items: AlignItems::FlexEnd,
                ..default()
            },
            z_index: ZIndex::Global(1),
            ..default()
        })
        .with_children(|parent| {
            parent.spawn(TextBundle::default());
        })
        .insert(ItemStack::default())
        .insert(CursorItemStackMarker);
}

// Add content to the interface sent from the server.
fn handle_interface_item_box_updates(
    mut commands: Commands,
    interfaces: Res<Interfaces>,
    net: Res<NetworkClient>,
    items: Res<Items>,
    interface_query: Query<&Interface>,
    // Option<Children> here because there's a delay between adding the interface and receiving the
    // item boxes.
    item_box_section_query: Query<(Entity, Option<&Children>), With<ItemBoxSectionMarker>>,
    mut interface_updates: EventReader<NetworkData<messages::InterfaceItemBoxUpdate>>,
) {
    for interface_update in interface_updates.iter() {
        let interface_entity = match interfaces.get(&interface_update.name) {
            Some(i) => *i,
            None => {
                net.disconnect(&format!(
                    "Server sent update for interface with name: {}, but there is no such interface defined.",
                    &interface_update.name
                ));
                return;
            }
        };
        let interface = interface_query.get(interface_entity).unwrap();

        // For each item box in the update we need to either get an existing or insert a new entity, then we
        // replace/insert the ui node
        for (section_id, item_box_updates) in interface_update.item_box_sections.iter() {
            let (section_entity, section_item_box_entities) = match item_box_section_query
                .get(interface.item_box_section_entities[*section_id as usize])
            {
                Ok(q) => q,
                Err(_) => {
                    net.disconnect(format!(
                        "Server sent interface section that doesn't exist. The section '{}', does not exist in the '{}' interface.",
                        section_id,
                        &interface.config.name)
                    );
                    return;
                }
            };

            // TODO: This breaks the interface. Item images dissapear. I think it is a bug in the
            // AssetServer, when all handles to an image is dropped, the image is unloaded. If a
            // new handle is then created it will not load the image again.
            if interface_update.replace {
                commands.entity(section_entity).despawn_descendants();
            }

            for (i, item_box_update) in item_box_updates.iter().enumerate() {
                let mut box_entity_commands = if interface_update.replace
                    || section_item_box_entities.is_none()
                {
                    if i != item_box_update.item_box_id as usize {
                        net.disconnect(format!(
                            "Server sent interface item box update in incorrect order. Interface \
                            '{}', section '{}', box '{}' was received, but the section only has '{}' boxes. \
                            All previous boxes need to be added first.",
                            &interface.config.name,
                            section_id,
                            item_box_update.item_box_id,
                            i)
                        );
                        return;
                    }
                    let mut entity_commands = commands.spawn_empty();
                    let entity = entity_commands.id();
                    // Add item box to section
                    entity_commands
                        .commands()
                        .entity(section_entity)
                        .add_child(entity);

                    // Image set in update_item_box_images
                    entity_commands
                        .insert(ImageBundle {
                            // TODO: This doesn't actually block? Can't highlight items because of it.
                            focus_policy: bevy::ui::FocusPolicy::Block,
                            style: Style {
                                size: Size::new(Val::Px(16.0), Val::Px(16.0)),
                                // https://github.com/bevyengine/bevy/issues/6879
                                //padding: UiRect {
                                //    left: Val::Px(1.0),
                                //    right: Val::Auto,
                                //    top: Val::Px(1.0),
                                //    bottom: Val::Auto,
                                //},
                                // puts item count text in the bottom right corner
                                flex_direction: FlexDirection::ColumnReverse,
                                align_items: AlignItems::FlexEnd,
                                ..default()
                            },
                            background_color: BackgroundColor(Color::NONE),
                            ..default()
                        })
                        .insert(Interaction::default())
                        .insert(ItemBox {
                            interface_entity,
                            section_index: *section_id as usize,
                            index: item_box_update.item_box_id as usize,
                        })
                        // Item text
                        .with_children(|parent| {
                            parent.spawn(TextBundle::default());
                        });

                    entity_commands
                } else {
                    let section_item_box_entities = section_item_box_entities.unwrap();

                    match section_item_box_entities.get(item_box_update.item_box_id as usize) {
                        Some(e) => commands.entity(*e),
                        None => {
                            net.disconnect(format!(
                                "Server sent malformed interface item box update. Interface \
                                '{}', section '{}', box '{}' was received, but the section only has \
                                '{}' boxes.",
                                &interface.config.name,
                                section_id,
                                item_box_update.item_box_id,
                                section_item_box_entities.len() + i)
                            );
                            return;
                        }
                    }
                };

                let item_stack = if let Some(item_id) = &item_box_update.item_stack.item_id {
                    let item_config = match items.configs.get(item_id) {
                        Some(i) => i,
                        None => {
                            net.disconnect(&format!(
                                "While updating the {} interface the server sent an unrecognized item id {}",
                                &interface_update.name,
                                item_id
                            ));
                            return;
                        }
                    };
                    ItemStack::new(
                        *item_id,
                        item_config.stack_size,
                        item_box_update.item_stack.quantity,
                    )
                } else {
                    ItemStack::default()
                };

                box_entity_commands.insert(item_stack);
            }
        }
    }
}

// Open/close interfaces on request from the client.
fn handle_interface_toggle_events(
    items: Res<Items>,
    net: Res<NetworkClient>,
    mut interface_stack: ResMut<InterfaceStack>,
    mut interface_toggle_events: EventReader<InterfaceToggleEvent>,
    mut interface_query: Query<(Entity, &Interface, &mut Visibility), With<Interface>>,
    item_box_section_query: Query<&Children, With<ItemBoxSectionMarker>>,
    mut held_item_stack_query: Query<
        &mut ItemStack,
        (With<CursorItemStackMarker>, Without<ItemBox>),
    >,
    mut item_box_query: Query<(&mut ItemStack, &ItemBox)>,
) {
    for event in interface_toggle_events.iter() {
        for (entity, interface, mut visibility) in interface_query.iter_mut() {
            if entity == event.0 {
                continue;
            } else if *visibility == Visibility::Visible {
                *visibility = Visibility::Hidden;

                if !interface.config.is_exclusive {
                    interface_stack.push(entity);
                }
            }
        }

        // Yes this could be inserted above, but there's so much indentation
        let (_, interface, mut visibility) = interface_query.get_mut(event.0).unwrap();

        if *visibility == Visibility::Visible {
            *visibility = Visibility::Hidden;

            // TODO: Which box it puts the item into is random-ish, it needs to be specific. First
            // check the box it was taken from, if that is full, choose the first available spot by
            // the order of the item box sections defined in the config.
            //
            // Put the item that is held back into the interface.
            let mut held_item_stack = held_item_stack_query.single_mut();
            if !held_item_stack.is_empty() {
                'outer: for (i, section_config) in
                    interface.config.item_box_sections.iter().enumerate()
                {
                    if let Some(allowed) = &section_config.allowed_item_types {
                        let item_config = items.get(&held_item_stack.item.unwrap());
                        if let Some(categories) = &item_config.categories {
                            if allowed.is_disjoint(categories) {
                                continue;
                            }
                        }
                    }

                    let section_entity = interface.item_box_section_entities[i];
                    for item_box_entity in item_box_section_query.iter_descendants(section_entity) {
                        let (mut item_stack, item_box) =
                            item_box_query.get_mut(item_box_entity).unwrap();
                        if item_stack.item == held_item_stack.item {
                            let size = held_item_stack.size;
                            let transfered = item_stack.transfer(&mut held_item_stack, size);

                            net.send_message(messages::InterfacePlaceItem {
                                name: interface.config.name.clone(),
                                section: i as u32,
                                to_box: item_box.index as u32,
                                quantity: transfered,
                            })
                        }

                        if held_item_stack.is_empty() {
                            break 'outer;
                        }
                    }

                    // Has to be split from above because we first want it to fill up any existing
                    // stacks before it begins on empty stacks.
                    for item_box_entity in item_box_section_query.iter_descendants(section_entity) {
                        let (mut item_stack, item_box) =
                            item_box_query.get_mut(item_box_entity).unwrap();
                        if item_stack.is_empty() {
                            let size = held_item_stack.size;
                            let transfered = item_stack.transfer(&mut held_item_stack, size);

                            net.send_message(messages::InterfacePlaceItem {
                                name: interface.config.name.clone(),
                                section: i as u32,
                                to_box: item_box.index as u32,
                                quantity: transfered,
                            })
                        }

                        if held_item_stack.is_empty() {
                            break 'outer;
                        }
                    }
                }
            }

            if let Some(entity) = interface_stack.pop() {
                let (_, _, mut visibility) = interface_query.get_mut(entity).unwrap();
                *visibility = Visibility::Visible;
            }
        } else {
            *visibility = Visibility::Visible;
        }
    }
}

// Open interfaces sent by the server.
fn handle_interface_open_request(
    interfaces: Res<Interfaces>,
    net: Res<NetworkClient>,
    mut interface_query: Query<&mut Visibility, With<Interface>>,
    mut interface_open_events: EventReader<NetworkData<messages::InterfaceOpen>>,
) {
    for event in interface_open_events.iter() {
        let interface_entity = match interfaces.get(&event.name) {
            Some(e) => e,
            None => {
                net.disconnect(&format!(
                    "Server sent an interface with name '{}', but there is no interface known by this name.",
                    event.name
                ));
                return;
            }
        };
        *interface_query.get_mut(*interface_entity).unwrap() = Visibility::Visible;
    }
}

fn handle_interface_close_request(
    interfaces: Res<Interfaces>,
    net: Res<NetworkClient>,
    mut interface_query: Query<&mut Visibility, With<Interface>>,
    mut interface_open_events: EventReader<NetworkData<messages::InterfaceClose>>,
) {
    for event in interface_open_events.iter() {
        let interface_entity = match interfaces.get(&event.name) {
            Some(e) => e,
            None => {
                net.disconnect(&format!(
                    "Server sent an interface with name '{}', but there is no interface known by this name.",
                    event.name
                ));
                return;
            }
        };
        *interface_query.get_mut(*interface_entity).unwrap() = Visibility::Hidden;
    }
}

// TODO: Interaction only supports Clicked, so can't distinguish between left and right click for
//       fancy placement without hacking.
fn item_box_mouse_interaction(
    mut commands: Commands,
    net: Res<NetworkClient>,
    items: Res<Items>,
    interface_query: Query<&Interface>,
    mouse_button_input: Res<Input<MouseButton>>,
    keyboard_input: Res<Input<KeyCode>>,
    mut item_box_query: Query<(&mut ItemStack, &Interaction, &ItemBox), Changed<Interaction>>,
    mut held_item_stack_query: Query<
        &mut ItemStack,
        (With<CursorItemStackMarker>, Without<ItemBox>),
    >,
    interaction_query: Query<(Entity, &Interaction), (Changed<Interaction>, With<ItemBox>)>,
    mut highlighted_item_box: Local<Option<Entity>>,
) {
    // TODO: This highlights, but there is a bug. Even though the highlight is spawned as a child
    // entity of the item box and the item box has FocusPolicy::Block, it still triggers a
    // Interaction change to Interaction::None, causing a loop.
    //
    //// Iterate through all interactions to make sure the item box which was left gets its
    //// highlight cleared before a new one is added. If we go box -> box, it might try to add before
    //// it is removed.
    //for (_, interaction) in interaction_query.iter() {
    //    if *interaction == Interaction::None {
    //        match highlighted_item_box.take() {
    //            // Have to do despawn_recursive here or it crashes for some reason.
    //            // Even though it has not children (???) Wanted just "despawn".
    //            // Perhaps related https://github.com/bevyengine/bevy/issues/267
    //            Some(e) => commands.entity(e).despawn_recursive(),
    //            None => (),
    //        };
    //    }
    //}
    //for (entity, interaction) in interaction_query.iter() {
    //    if *interaction == Interaction::Hovered {
    //        *highlighted_item_box = Some(commands.entity(entity).add_children(|parent| {
    //            parent
    //                .spawn_bundle(NodeBundle {
    //                    style: Style {
    //                        position_type: PositionType::Absolute,
    //                        size: Size {
    //                            width: Val::Px(16.0),
    //                            height: Val::Px(16.0),
    //                        },
    //                        ..default()
    //                    },
    //                    color: UiColor(Color::Rgba {
    //                        red: 1.0,
    //                        green: 1.0,
    //                        blue: 1.0,
    //                        alpha: 0.7,
    //                    }),
    //                    ..default()
    //                })
    //                .id()
    //        }));
    //        //dbg!(highlighted_item_box.unwrap());
    //    }
    //}

    // TODO: It should only pick up when the button is released. But the clicked Interaction does
    // not sync up with just_released, only just_pressed
    for (mut box_item_stack, interaction, item_box) in item_box_query.iter_mut() {
        if *interaction != Interaction::Clicked {
            return;
        }
        if mouse_button_input.just_pressed(MouseButton::Left)
            && !keyboard_input.pressed(KeyCode::LShift)
        {
            let mut held_item_stack = held_item_stack_query.single_mut();
            let interface = interface_query.get(item_box.interface_entity).unwrap();

            if held_item_stack.is_empty() && !box_item_stack.is_empty() {
                // Take item from box
                let item_config = items.get(&box_item_stack.item.unwrap());

                held_item_stack.transfer(&mut box_item_stack, item_config.stack_size);

                net.send_message(messages::InterfaceTakeItem {
                    name: interface.config.name.clone(),
                    section: item_box.section_index as u32,
                    from_box: item_box.index as u32,
                    quantity: held_item_stack.size,
                })
            } else if !held_item_stack.is_empty() {
                // place held item, swap if box is not empty
                let section_config = &interface.config.item_box_sections[item_box.section_index];
                let item_config = items.get(&held_item_stack.item.unwrap());

                if let Some(allowed) = &section_config.allowed_item_types {
                    if let Some(categories) = &item_config.categories {
                        if allowed.is_disjoint(categories) {
                            continue;
                        }
                    } else {
                        continue;
                    }
                }

                // TODO: When used directly in the function the borrow checker say bad, even though
                // good
                let size = held_item_stack.size;
                let transfered = box_item_stack.transfer(&mut held_item_stack, size);

                net.send_message(messages::InterfacePlaceItem {
                    name: interface.config.name.clone(),
                    section: item_box.section_index as u32,
                    to_box: item_box.index as u32,
                    quantity: transfered,
                })
            }
        }

        if mouse_button_input.just_pressed(MouseButton::Left)
            && keyboard_input.pressed(KeyCode::LShift)
        {
            let mut held_item_stack = held_item_stack_query.single_mut();
            let interface = interface_query.get(item_box.interface_entity).unwrap();
            let section_config = &interface.config.item_box_sections[item_box.section_index];

            if held_item_stack.is_empty() && !box_item_stack.is_empty() {
                // TODO: This is a special condition for item boxes that are considered
                // output-only. e.g. crafting output. Given all the different actions that can
                // be intended by a click I think it should be configured through the interface
                // config. (Some key combo) -> "place/take" etc
                let transfered = if section_config.allowed_item_types.is_some()
                    && section_config
                        .allowed_item_types
                        .as_ref()
                        .unwrap()
                        .is_empty()
                {
                    let size = box_item_stack.size;
                    held_item_stack.transfer(&mut box_item_stack, size)
                } else {
                    // If even take half, if odd take half + 1
                    let size = (box_item_stack.size + 1) / 2;
                    held_item_stack.transfer(&mut box_item_stack, size)
                };

                net.send_message(messages::InterfaceTakeItem {
                    name: interface.config.name.clone(),
                    section: item_box.section_index as u32,
                    from_box: item_box.index as u32,
                    quantity: transfered,
                })
            } else if !held_item_stack.is_empty() {
                // place held item, swap if box is not empty
                let section_config = &interface.config.item_box_sections[item_box.section_index];
                let item_config = items.get(&held_item_stack.item.unwrap());

                if section_config.allowed_item_types.is_some()
                    && section_config
                        .allowed_item_types
                        .as_ref()
                        .unwrap()
                        .is_empty()
                    && held_item_stack.item == box_item_stack.item
                {
                    let size = box_item_stack.size;
                    let transfered = held_item_stack.transfer(&mut box_item_stack, size);
                    net.send_message(messages::InterfaceTakeItem {
                        name: interface.config.name.clone(),
                        section: item_box.section_index as u32,
                        from_box: item_box.index as u32,
                        quantity: transfered,
                    });
                } else {
                    if let Some(allowed) = &section_config.allowed_item_types {
                        if let Some(categories) = &item_config.categories {
                            if allowed.is_disjoint(categories) {
                                continue;
                            }
                        } else {
                            continue;
                        }
                    }

                    let transfered = box_item_stack.transfer(&mut held_item_stack, 1);

                    net.send_message(messages::InterfacePlaceItem {
                        name: interface.config.name.clone(),
                        section: item_box.section_index as u32,
                        to_box: item_box.index as u32,
                        quantity: transfered,
                    })
                };
            }
        }
    }
}

fn update_cursor_item_stack_position(
    ui_scale: Res<UiScale>,
    mut cursor_move_event: EventReader<CursorMoved>,
    mut held_item_stack_query: Query<&mut Style, With<CursorItemStackMarker>>,
) {
    for cursor_movement in cursor_move_event.iter() {
        let mut style = held_item_stack_query.single_mut();
        style.left = Val::Px(cursor_movement.position.x / ui_scale.scale as f32 - 8.0);
        style.top = Val::Px(cursor_movement.position.y / ui_scale.scale as f32 - 8.0);
    }
}

fn update_item_box_images(
    asset_server: Res<AssetServer>,
    items: Res<Items>,
    mut item_box_query: Query<
        (&mut UiImage, &ItemStack, &mut BackgroundColor, &Children),
        (With<ItemBox>, Changed<ItemStack>),
    >,
    mut cursor_item_query: Query<
        (&mut UiImage, &ItemStack, &mut BackgroundColor, &Children),
        (
            With<CursorItemStackMarker>,
            Without<ItemBox>,
            Changed<ItemStack>,
        ),
    >,
    // TODO: I think there's something about relations being added, this can have parent ==
    // Itembox/CursorItemStackMarker if that will be possible
    mut text_query: Query<&mut Text>,
) {
    let mut update_image = |image: &mut UiImage,
                            item_stack: &ItemStack,
                            color: &mut BackgroundColor,
                            children: &Children| {
        if let Some(item_id) = item_stack.item {
            *image = asset_server.load(&items.get(&item_id).image_path).into();
            *color = BackgroundColor(Color::WHITE);

            let mut text = text_query.get_mut(children[0]).unwrap();
            *text = Text::from_section(
                item_stack.size.to_string(),
                TextStyle {
                    font: asset_server.load("server_assets/font.otf"),
                    font_size: 6.0,
                    color: if item_stack.size > 1 {
                        Color::WHITE
                    } else {
                        Color::NONE
                    },
                },
            );
        } else {
            // Instead of hiding the node through visibility we mask it with the color. This is
            // because the item box still needs to be interacable so items can be put into it.
            *color = BackgroundColor(Color::NONE);
            let mut text = text_query.get_mut(children[0]).unwrap();
            *text = Text::from_section(
                item_stack.size.to_string(),
                TextStyle {
                    font: asset_server.load("server_assets/font.otf"),
                    font_size: 6.0,
                    color: Color::NONE,
                },
            );
        }
    };

    for (mut image, item_stack, mut color, children) in item_box_query.iter_mut() {
        update_image(&mut image, item_stack, &mut color, children);
    }

    for (mut image, item_stack, mut color, children) in cursor_item_query.iter_mut() {
        update_image(&mut image, item_stack, &mut color, children);
    }
}

fn cursor_visibility(
    mut window: Query<&mut Window, With<PrimaryWindow>>,
    changed_interfaces: Query<(&Interface, &Visibility), Changed<Visibility>>,
) {
    for (interface, visibility) in changed_interfaces.iter() {
        if interface.item_box_section_entities.len() > 0 && !interface.config.is_equipment {
            let mut window = window.single_mut();

            if visibility == Visibility::Visible {
                window.cursor.visible = true;
                let position = Vec2::new(window.width() / 2.0, window.height() / 2.0);
                window.set_cursor_position(Some(position));
                window.cursor.grab_mode = CursorGrabMode::None;
            } else {
                window.cursor.visible = false;
                window.cursor.grab_mode = if cfg!(unix) {
                    CursorGrabMode::Locked
                } else {
                    CursorGrabMode::Confined
                };
            }
        }
    }
}

// All interfaces with this component will render an outline around the selected item.
// An item is always selected, and it persists on open/close of the interface.
#[derive(Component)]
pub struct SelectedItemBox(pub Entity);

// Add a SelectedItemBox component to all interfaces that have item boxes.
fn initial_select_item_box(
    mut commands: Commands,
    added_itembox_query: Query<(Entity, &ItemBox), Added<ItemBox>>,
) {
    for (box_entity, item_box) in added_itembox_query.iter() {
        if item_box.index == 0 {
            commands
                .entity(item_box.interface_entity)
                .insert(SelectedItemBox(box_entity));
        }
    }
}

fn keyboard_select_item_box(
    keyboard: Res<Input<KeyCode>>,
    mut interface_query: Query<(&Interface, &Visibility, &mut SelectedItemBox)>,
    item_box_section_query: Query<Option<&Children>, With<ItemBoxSectionMarker>>,
) {
    for key in keyboard.get_just_pressed() {
        for (interface, visibility, mut selected) in interface_query.iter_mut() {
            if visibility == Visibility::Hidden {
                continue;
            }
            // Make sure the interface has an item box section, and that the item boxes have been
            // received from the server.
            let children = if let Some(section_entity) = interface.item_box_section_entities.get(0)
            {
                match item_box_section_query.get(*section_entity).unwrap() {
                    Some(children) => children,
                    None => continue,
                }
            } else {
                continue;
            };

            *selected = match key {
                KeyCode::Key1 => match children.get(0) {
                    Some(entity) => SelectedItemBox(*entity),
                    None => continue,
                },
                KeyCode::Key2 => match children.get(1) {
                    Some(entity) => SelectedItemBox(*entity),
                    None => continue,
                },
                KeyCode::Key3 => match children.get(2) {
                    Some(entity) => SelectedItemBox(*entity),
                    None => continue,
                },
                KeyCode::Key4 => match children.get(3) {
                    Some(entity) => SelectedItemBox(*entity),
                    None => continue,
                },
                KeyCode::Key5 => match children.get(4) {
                    Some(entity) => SelectedItemBox(*entity),
                    None => continue,
                },
                KeyCode::Key6 => match children.get(5) {
                    Some(entity) => SelectedItemBox(*entity),
                    None => continue,
                },
                KeyCode::Key7 => match children.get(6) {
                    Some(entity) => SelectedItemBox(*entity),
                    None => continue,
                },
                KeyCode::Key8 => match children.get(7) {
                    Some(entity) => SelectedItemBox(*entity),
                    None => continue,
                },
                KeyCode::Key9 => match children.get(8) {
                    Some(entity) => SelectedItemBox(*entity),
                    None => continue,
                },
                _ => continue,
            };
        }
    }
}
