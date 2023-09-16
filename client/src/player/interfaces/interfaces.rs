use std::collections::{HashMap, HashSet};

use bevy::{
    ecs::system::EntityCommands,
    prelude::*,
    render::texture::CompressedImageFormats,
    window::{CursorGrabMode, PrimaryWindow},
};

use fmc_networking::{messages, NetworkClient, NetworkData};
use serde::{Deserialize, Serialize};

use crate::game_state::GameState;

use super::items::{ItemConfig, ItemStack, Items};

const INTERFACE_CONFIG_PATH: &str = "server_assets/interfaces/";
const INTERFACE_TEXTURE_PATH: &str = "server_assets/textures/interfaces/";

// TODO: I decided to use "take/place" instead of "swap" to move items around interfaces. This was
// to limit the frustration of the players if several were to use the same interface at once. This
// would lead to "stealing" items from each other when several hold the same item. I'm beginning to
// think it was a bad idea. It would have simpler code in exchange for some slightly weird
// behaviour. Do a think about this.
//
// TODO: The item grid in the inventory can only have 7 columns, if more the layout breaks.
pub struct InterfacePlugin;
impl Plugin for InterfacePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(InterfaceStack::default())
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

// A map from 'InterfacePath' to entity.
#[derive(Resource, Deref, DerefMut, Default)]
pub struct Interfaces(HashMap<String, Entity>);

// Nodes of an interface that can be interacted with are referenced by their path in the ui
// hierarchy. The path is formatted "parent/child/grandchild" by the names given in the config. For
// example an inventory will have a root node named "inventory" with sections named
// "inventory/equipment", "inventory/crafting", "inventory/storage" etc.
// When interacting with an interface e.g. moving items, this name is sent to the server to
// identify which interface and which section is being interacted with.
#[derive(Component)]
pub struct InterfacePath(pub String);

// Marker struct only for interface roots.
#[derive(Component)]
struct InterfaceRoot;

/// Event used by keybindings to toggle an interface open or closed.
#[derive(Event)]
pub struct InterfaceToggleEvent(pub Entity);

#[derive(Component)]
pub struct ItemBox {
    pub item_stack: ItemStack,
    // Box index in the section
    pub index: usize,
}

impl ItemBox {
    fn is_empty(&self) -> bool {
        self.item_stack.is_empty()
    }
}

#[derive(Deserialize, Component, Clone)]
#[serde(default, deny_unknown_fields)]
pub struct ItemBoxSection {
    /// If it is allowed to quick move to this section.
    allow_quick_place: bool,
    /// Which item types can be placed in this section.
    allowed_item_types: Option<HashSet<String>>,
    /// If the items can be moved by mouse/keyboard interaction
    movable_items: bool,
    /// Whether items should be equipped by the hand on selection.
    #[serde(rename = "equipment")]
    pub is_equipment: bool,
}

impl ItemBoxSection {
    fn can_contain(&self, item_config: &ItemConfig) -> bool {
        if let Some(allowed) = &self.allowed_item_types {
            if let Some(categories) = &item_config.categories {
                if allowed.is_disjoint(categories) {
                    false
                } else {
                    true
                }
            } else {
                false
            }
        } else {
            true
        }
    }
}

impl Default for ItemBoxSection {
    fn default() -> Self {
        Self {
            allow_quick_place: false,
            allowed_item_types: None,
            movable_items: true,
            is_equipment: false,
        }
    }
}

#[derive(Default, Deserialize, Component, Clone)]
#[serde(default, deny_unknown_fields)]
struct NodeConfig {
    /// Optional name, the server uses this when referring to an interfaces.
    name: Option<String>,
    /// Style used to render the interface node.
    style: NodeStyle,
    /// Content contained by the node.
    content: NodeContent,
    /// Image displayed in the node.
    image: Option<String>,
    /// If it should overlap(non exclusive) or replace(exclusive) interfaces when opened.
    #[serde(rename = "exclusive", default)]
    pub is_exclusive: bool,
}

// TODO: As json I want to to be like "content: [..]" for Nodes, but all others should be
// adjacently tagged like "content: {type: item_box_section, fields...}". Maybe the adjacently
// tagged ones need to be another enum that is wrapped by one of this enums variants.
#[derive(Default, Deserialize, Clone)]
enum NodeContent {
    #[default]
    None,
    Nodes(Vec<NodeConfig>),
    ItemSection(ItemBoxSection),
}

#[derive(Deserialize, Default, Clone, Debug)]
#[serde(default)]
struct Rect {
    left: Option<Val>,
    right: Option<Val>,
    top: Option<Val>,
    bottom: Option<Val>,
}

impl From<Rect> for UiRect {
    fn from(value: Rect) -> Self {
        UiRect {
            left: value.left.unwrap_or(Val::Px(0.0)),
            right: value.right.unwrap_or(Val::Px(0.0)),
            top: value.top.unwrap_or(Val::Px(0.0)),
            bottom: value.bottom.unwrap_or(Val::Px(0.0)),
        }
    }
}

// TODO: Maybe open issue in bevy see if Style can be made de/se. Missing for UiRect, but
// the rest have it.
//
// Deserializable Style
#[derive(Deserialize, Clone, Debug)]
#[serde(default, deny_unknown_fields)]
struct NodeStyle {
    pub display: Display,
    pub position_type: PositionType,
    pub overflow: Overflow,
    pub direction: Direction,
    pub left: Val,
    pub right: Val,
    pub top: Val,
    pub bottom: Val,
    pub width: Val,
    pub height: Val,
    pub min_width: Val,
    pub min_height: Val,
    pub max_width: Val,
    pub max_height: Val,
    pub aspect_ratio: Option<f32>,
    pub align_items: AlignItems,
    pub justify_items: JustifyItems,
    pub align_self: AlignSelf,
    pub justify_self: JustifySelf,
    pub align_content: AlignContent,
    pub justify_content: JustifyContent,
    pub margin: Rect,
    pub padding: Rect,
    pub border: Rect,
    pub flex_direction: FlexDirection,
    pub flex_wrap: FlexWrap,
    pub flex_grow: f32,
    pub flex_shrink: f32,
    pub flex_basis: Val,
    pub row_gap: Val,
    pub column_gap: Val,
    pub grid_auto_flow: GridAutoFlow,
    pub grid_template_rows: Vec<RepeatedGridTrack>,
    pub grid_template_columns: Vec<RepeatedGridTrack>,
    pub grid_auto_rows: Vec<GridTrack>,
    pub grid_auto_columns: Vec<GridTrack>,
    pub grid_row: GridPlacement,
    pub grid_column: GridPlacement,
}

impl Default for NodeStyle {
    fn default() -> Self {
        Self {
            display: Display::DEFAULT,
            position_type: PositionType::default(),
            left: Val::Auto,
            right: Val::Auto,
            top: Val::Auto,
            bottom: Val::Auto,
            direction: Direction::default(),
            flex_direction: FlexDirection::default(),
            flex_wrap: FlexWrap::default(),
            align_items: AlignItems::default(),
            justify_items: JustifyItems::DEFAULT,
            align_self: AlignSelf::DEFAULT,
            justify_self: JustifySelf::DEFAULT,
            align_content: AlignContent::DEFAULT,
            justify_content: JustifyContent::DEFAULT,
            margin: Rect::default(),
            padding: Rect::default(),
            border: Rect::default(),
            flex_grow: 0.0,
            flex_shrink: 1.0,
            flex_basis: Val::Auto,
            width: Val::Auto,
            height: Val::Auto,
            min_width: Val::Auto,
            min_height: Val::Auto,
            max_width: Val::Auto,
            max_height: Val::Auto,
            aspect_ratio: None,
            overflow: Overflow::DEFAULT,
            row_gap: Val::Px(0.0),
            column_gap: Val::Px(0.0),
            grid_auto_flow: GridAutoFlow::default(),
            grid_template_rows: Vec::new(),
            grid_template_columns: Vec::new(),
            grid_auto_rows: Vec::new(),
            grid_auto_columns: Vec::new(),
            grid_column: GridPlacement::default(),
            grid_row: GridPlacement::default(),
        }
    }
}

impl From<NodeStyle> for Style {
    fn from(value: NodeStyle) -> Self {
        Style {
            display: value.display,
            position_type: value.position_type,
            overflow: value.overflow,
            direction: value.direction,
            left: value.left,
            right: value.right,
            top: value.top,
            bottom: value.bottom,
            width: value.width,
            height: value.height,
            min_width: value.min_width,
            min_height: value.min_height,
            max_width: value.max_width,
            max_height: value.max_height,
            aspect_ratio: value.aspect_ratio,
            align_items: value.align_items,
            justify_items: value.justify_items,
            align_self: value.align_self,
            justify_self: value.justify_self,
            align_content: value.align_content,
            justify_content: value.justify_content,
            margin: value.margin.into(),
            padding: value.padding.into(),
            border: value.border.into(),
            flex_direction: value.flex_direction,
            flex_wrap: value.flex_wrap,
            flex_grow: value.flex_grow,
            flex_shrink: value.flex_shrink,
            flex_basis: value.flex_basis,
            row_gap: value.row_gap,
            column_gap: value.column_gap,
            grid_auto_flow: value.grid_auto_flow,
            grid_template_rows: value.grid_template_rows,
            grid_template_columns: value.grid_template_columns,
            grid_auto_rows: value.grid_auto_rows,
            grid_auto_columns: value.grid_auto_columns,
            grid_row: value.grid_row,
            grid_column: value.grid_column,
        }
    }
}

// Interfaces that are open and allow other interfaces to take focus are stored here while the
// focused one is visible.
#[derive(Resource, Deref, DerefMut, Default)]
struct InterfaceStack(Vec<Entity>);

// The item stack is unique, and shared between all interfaces(since only one can be open at a
// time). When the interface is closed, the item is returned to the interface it was taken from.
#[derive(Component, Default)]
struct CursorItemBox {
    item_stack: ItemStack,
}

impl CursorItemBox {
    fn is_empty(&self) -> bool {
        self.item_stack.is_empty()
    }
}

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
                    "Misconfigured resource pack: Failed to open interface configuration at: '{}'\n\
                    Error: {}",
                    &file_path.display(),
                    e
                ));
                return;
            }
        };
        let node_config: NodeConfig = match serde_json::from_reader(&file) {
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

        // NOTE(WORKAROUND): When spawning an ImageBundle, the dimensions of the image are
        // inferred, but if it has children it's discarded and it uses the size of the children
        // instead. Images must therefore be spawned with defined width/height to display correctly.
        fn read_image_dimensions(image_path: &str) -> Vec2 {
            let image_data = match std::fs::read(INTERFACE_TEXTURE_PATH.to_owned() + image_path) {
                Ok(i) => i,
                Err(_) => {
                    return Vec2::ZERO;
                }
            };

            let image = match Image::from_buffer(
                &image_data,
                bevy::render::texture::ImageType::Extension("png"),
                CompressedImageFormats::NONE,
                false,
            ) {
                Ok(i) => i,
                Err(_) => {
                    return Vec2::ZERO;
                }
            };

            return image.size();
        }

        // TODO: The server needs to validate that no interfaces share a name. The client doesn't
        // need to care, it will just overwrite. It is hard to do with this recursion too.
        fn spawn_interface(
            entity_commands: &mut EntityCommands,
            parent_path: String,
            config: &NodeConfig,
            interfaces: &mut Interfaces,
            asset_server: &AssetServer,
        ) {
            let path = if let Some(interface_name) = &config.name {
                let path = if parent_path == "" {
                    interface_name.to_owned()
                } else {
                    parent_path + "/" + interface_name
                };

                entity_commands.insert(InterfacePath(path.clone()));
                interfaces.insert(path.clone(), entity_commands.id());

                path
            } else {
                parent_path
            };

            let style = if let Some(image_path) = &config.image {
                let dimensions = read_image_dimensions(&image_path);
                let mut style = Style::from(config.style.clone());
                style.width = Val::Px(dimensions.x);
                style.height = Val::Px(dimensions.y);
                style
            } else {
                config.style.clone().into()
            };

            entity_commands.insert((
                ImageBundle {
                    style,
                    background_color: config
                        .image
                        .as_ref()
                        .map_or(Color::NONE.into(), |_| Color::WHITE.into()),
                    image: config.image.as_ref().map_or(UiImage::default(), |path| {
                        asset_server
                            .load(INTERFACE_TEXTURE_PATH.to_owned() + &path)
                            .into()
                    }),
                    ..default()
                },
                config.clone(),
            ));

            match &config.content {
                NodeContent::Nodes(nodes) => {
                    entity_commands.with_children(|parent| {
                        for child_config in nodes.iter() {
                            let mut parent_entity_commands = parent.spawn_empty();
                            spawn_interface(
                                &mut parent_entity_commands,
                                path.clone(),
                                child_config,
                                interfaces,
                                asset_server,
                            )
                        }
                    });
                }
                NodeContent::ItemSection(section) => {
                    entity_commands.insert(section.clone());
                }
                NodeContent::None => ()
            }
        }

        commands
            .spawn(NodeBundle {
                style: Style {
                    position_type: PositionType::Absolute,
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
                ..default()
            })
            .with_children(|parent| {
                let mut entity_commands = parent.spawn_empty();
                spawn_interface(
                    &mut entity_commands,
                    String::new(),
                    &node_config,
                    &mut interfaces,
                    &asset_server,
                );

                entity_commands.insert((
                    InterfaceRoot,
                    VisibilityBundle {
                        visibility: Visibility::Hidden,
                        ..default()
                    },
                ));
            });
    }

    commands.insert_resource(interfaces);

    commands
        .spawn((
            ImageBundle {
                style: Style {
                    width: Val::Px(15.0),
                    height: Val::Px(16.0),
                    position_type: PositionType::Absolute,
                    flex_direction: FlexDirection::ColumnReverse,
                    align_items: AlignItems::FlexEnd,
                    ..default()
                },
                z_index: ZIndex::Global(1),
                ..default()
            },
            CursorItemBox::default(),
        ))
        .with_children(|parent| {
            parent.spawn(TextBundle::default());
        });
}

// Add content to the interface sent from the server.
fn handle_interface_item_box_updates(
    mut commands: Commands,
    interfaces: Res<Interfaces>,
    net: Res<NetworkClient>,
    items: Res<Items>,
    interface_item_box_query: Query<Option<&Children>, With<ItemBoxSection>>,
    mut item_box_update_events: EventReader<NetworkData<messages::InterfaceItemBoxUpdate>>,
) {
    for item_box_update in item_box_update_events.read() {
        for (interface_name, new_item_boxes) in item_box_update.updates.iter() {
            let interface_entity = match interfaces.get(interface_name) {
                Some(i) => *i,
                None => {
                    net.disconnect(&format!(
                        "Server sent item box update for interface with name: {}, but there is no interface by that name.",
                        &interface_name
                    ));
                    return;
                }
            };

            let children = match interface_item_box_query.get(interface_entity)
            {
                Ok(c) => c,
                Err(_) => {
                    net.disconnect(&format!(
                        "Server sent item box update for interface with name: {}, but the interface is not configured to contain item boxes.",
                        &interface_name
                    ));
                    return;
                }
            };

            // TODO: This breaks the interface. Item images dissapear. I think it is a bug in the
            // AssetServer, when all handles to an image are dropped, the image is unloaded. If a
            // new handle is then created it will not load the image again.
            if item_box_update.replace {
                commands.entity(interface_entity).despawn_descendants();
            }

            for item_box in new_item_boxes.iter() {
                let item_stack = if let Some(item_id) = &item_box.item_stack.item_id {
                    let item_config = match items.configs.get(item_id) {
                        Some(i) => i,
                        None => {
                            net.disconnect(&format!(
                                "While updating the '{}' interface the server sent an unrecognized item id {}",
                                &interface_name,
                                item_id
                            ));
                            return;
                        }
                    };
                    ItemStack::new(
                        *item_id,
                        item_config.stack_size,
                        item_box.item_stack.quantity,
                    )
                } else {
                    ItemStack::default()
                };

                let mut entity_commands = if item_box_update.replace || children.is_none() {
                    let mut entity_commands = commands.spawn_empty();
                    entity_commands.set_parent(interface_entity);
                    entity_commands
                } else if let Some(child_entity) = children.unwrap().get(item_box.index as usize) {
                    let mut entity_commands = commands.entity(*child_entity);
                    entity_commands.despawn_descendants();
                    entity_commands
                } else {
                    let mut entity_commands = commands.spawn_empty();
                    entity_commands.set_parent(interface_entity);
                    entity_commands
                };


                entity_commands
                    .insert(ImageBundle {
                        // TODO: This doesn't actually block? Can't highlight items because of it.
                        focus_policy: bevy::ui::FocusPolicy::Block,
                        style: Style {
                            width: Val::Px(15.0),
                            height: Val::Px(15.8),
                            margin: UiRect {
                                left: Val::Px(0.5),
                                right: Val::Px(0.5),
                                top: Val::Px(0.1),
                                bottom: Val::Px(0.1)
                            },
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
                        item_stack,
                        index: item_box.index as usize,
                    })
                    // Item text
                    .with_children(|parent| {
                        parent.spawn(TextBundle::default());
                    });
            }
        }
    }
}

// Open/close interfaces on request from the client.
fn handle_interface_toggle_events(
    items: Res<Items>,
    net: Res<NetworkClient>,
    mut interface_stack: ResMut<InterfaceStack>,
    mut interface_query: Query<
        (Entity, &NodeConfig, &mut Visibility),
        With<InterfaceRoot>,
    >,
    item_box_section_query: Query<(
        &ItemBoxSection,
        Option<&Children>,
        &ViewVisibility,
        &InterfacePath,
    )>,
    mut cursor_item_box_query: Query<&mut CursorItemBox>,
    mut item_box_query: Query<&mut ItemBox>,
    mut interface_toggle_events: EventReader<InterfaceToggleEvent>,
) {
    for event in interface_toggle_events.read() {
        let mut cursor_box = cursor_item_box_query.single_mut();
        if !cursor_box.is_empty() {
            'outer: for (item_box_section, children, view_visibility, interface_path) in
                item_box_section_query.iter()
            {
                // Test that the item box section is part of the currently open interface
                if !view_visibility.get() {
                    continue;
                }

                let item_config = items.get(&cursor_box.item_stack.item.unwrap());
                if !item_box_section.can_contain(item_config) {
                    continue;
                }

                if let Some(children) = children {
                    for item_box_entity in children.iter() {
                        let mut item_box = item_box_query.get_mut(*item_box_entity).unwrap();
                        if item_box.item_stack.item == cursor_box.item_stack.item {
                            let transfered = item_box
                                .item_stack
                                .transfer(&mut cursor_box.item_stack, u32::MAX);
                            net.send_message(messages::InterfacePlaceItem {
                                interface_path: interface_path.0.clone(),
                                to_box: item_box.index as u32,
                                quantity: transfered,
                            })
                        }

                        if cursor_box.is_empty() {
                            break 'outer;
                        }
                    }

                    // Has to be split from above because we first want it to fill up any existing
                    // stacks before it begins on empty stacks.
                    for item_box_entity in children.iter() {
                        let mut item_box = item_box_query.get_mut(*item_box_entity).unwrap();
                        if item_box.is_empty() {
                            let transfered = item_box
                                .item_stack
                                .transfer(&mut cursor_box.item_stack, u32::MAX);
                            net.send_message(messages::InterfacePlaceItem {
                                interface_path: interface_path.0.clone(),
                                to_box: item_box.index as u32,
                                quantity: transfered,
                            });

                            break 'outer;
                        }
                    }
                }
            }
        }

        let mut was_exclusive = false;
        for (entity, interface_config, mut visibility) in interface_query.iter_mut() {
            if entity == event.0 {
                if *visibility == Visibility::Visible {
                    *visibility = Visibility::Hidden;
                    was_exclusive = interface_config.is_exclusive;
                } else {
                    *visibility = Visibility::Visible;
                }
            } else if *visibility == Visibility::Visible {
                *visibility = Visibility::Hidden;

                if !interface_config.is_exclusive {
                    interface_stack.push(entity);
                }
            }
        }

        if was_exclusive {
            for interface_entity in interface_stack.drain(..) {
                let (_, _, mut visibility) = interface_query.get_mut(interface_entity).unwrap();
                *visibility = Visibility::Visible;
            }
        }
    }
}

// Open interfaces sent by the server.
fn handle_interface_open_request(
    interfaces: Res<Interfaces>,
    net: Res<NetworkClient>,
    mut interface_query: Query<&mut Visibility, With<InterfaceRoot>>,
    mut interface_open_events: EventReader<NetworkData<messages::InterfaceOpen>>,
) {
    for event in interface_open_events.read() {
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
    mut interface_query: Query<&mut Visibility, With<InterfaceRoot>>,
    mut interface_open_events: EventReader<NetworkData<messages::InterfaceClose>>,
) {
    for event in interface_open_events.read() {
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
    mouse_button_input: Res<Input<MouseButton>>,
    keyboard_input: Res<Input<KeyCode>>,
    item_box_section_query: Query<(&ItemBoxSection, &InterfacePath)>,
    mut item_box_query: Query<(&mut ItemBox, &Interaction, &Parent), Changed<Interaction>>,
    mut cursor_item_box_query: Query<&mut CursorItemBox>,
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
    for (mut item_box, interaction, parent) in item_box_query.iter_mut() {
        if *interaction != Interaction::Pressed {
            return;
        }
        if mouse_button_input.just_pressed(MouseButton::Left)
            && !keyboard_input.pressed(KeyCode::ShiftLeft)
        {
            let mut cursor_box = cursor_item_box_query.single_mut();
            let (item_box_section, interface_path) =
                item_box_section_query.get(parent.get()).unwrap();

            if cursor_box.is_empty() && !item_box.is_empty() {
                // Take item from box
                let item_config = items.get(&item_box.item_stack.item.unwrap());

                let transfered = cursor_box
                    .item_stack
                    .transfer(&mut item_box.item_stack, item_config.stack_size);

                net.send_message(messages::InterfaceTakeItem {
                    interface_path: interface_path.0.clone(),
                    from_box: item_box.index as u32,
                    quantity: transfered,
                })
            } else if !cursor_box.is_empty() {
                // place held item, swap if box is not empty
                let item_config = items.get(&cursor_box.item_stack.item.unwrap());

                if !item_box_section.can_contain(item_config) {
                    continue;
                }

                // TODO: When used directly in the function the borrow checker say bad, even though
                // good
                let size = cursor_box.item_stack.size;
                let transfered = item_box
                    .item_stack
                    .transfer(&mut cursor_box.item_stack, size);

                net.send_message(messages::InterfacePlaceItem {
                    interface_path: interface_path.0.clone(),
                    to_box: item_box.index as u32,
                    quantity: transfered,
                })
            }
        }

        if mouse_button_input.just_pressed(MouseButton::Left)
            && keyboard_input.pressed(KeyCode::ShiftLeft)
        {
            let mut cursor_box = cursor_item_box_query.single_mut();
            let (item_box_section, interface_path) =
                item_box_section_query.get(parent.get()).unwrap();

            if cursor_box.is_empty() && !item_box.is_empty() {
                // TODO: This is a special condition for item boxes that are considered
                // output-only. e.g. crafting output. Given all the different actions that can
                // be intended by a click I think it should be configured through the interface
                // config. (Some key combo) -> "place/take" etc
                let transfered = if item_box_section.allowed_item_types.is_some()
                    && item_box_section
                        .allowed_item_types
                        .as_ref()
                        .unwrap()
                        .is_empty()
                {
                    let size = item_box.item_stack.size;
                    cursor_box
                        .item_stack
                        .transfer(&mut item_box.item_stack, size)
                } else {
                    // If even take half, if odd take half + 1
                    let size = (item_box.item_stack.size + 1) / 2;
                    cursor_box
                        .item_stack
                        .transfer(&mut item_box.item_stack, size)
                };

                net.send_message(messages::InterfaceTakeItem {
                    interface_path: interface_path.0.clone(),
                    from_box: item_box.index as u32,
                    quantity: transfered,
                })
            } else if !cursor_box.is_empty() {
                // place held item, swap if box is not empty
                let item_config = items.get(&cursor_box.item_stack.item.unwrap());

                if item_box_section.allowed_item_types.is_some()
                    && item_box_section
                        .allowed_item_types
                        .as_ref()
                        .unwrap()
                        .is_empty()
                    && cursor_box.item_stack.item == item_box.item_stack.item
                {
                    let size = item_box.item_stack.size;
                    let transfered = cursor_box
                        .item_stack
                        .transfer(&mut item_box.item_stack, size);
                    net.send_message(messages::InterfaceTakeItem {
                        interface_path: interface_path.0.clone(),
                        from_box: item_box.index as u32,
                        quantity: transfered,
                    });
                } else {
                    if !item_box_section.can_contain(item_config) {
                        continue;
                    }

                    let transfered = item_box.item_stack.transfer(&mut cursor_box.item_stack, 1);

                    net.send_message(messages::InterfacePlaceItem {
                        interface_path: interface_path.0.clone(),
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
    mut held_item_stack_query: Query<&mut Style, With<CursorItemBox>>,
) {
    for cursor_movement in cursor_move_event.read() {
        let mut style = held_item_stack_query.single_mut();
        style.left = Val::Px(cursor_movement.position.x / ui_scale.0 as f32 - 8.0);
        style.top = Val::Px(cursor_movement.position.y / ui_scale.0 as f32 - 8.0);
    }
}

fn update_item_box_images(
    asset_server: Res<AssetServer>,
    items: Res<Items>,
    mut item_box_query: Query<
        (&mut UiImage, &ItemBox, &mut BackgroundColor, &Children),
        (Changed<ItemBox>, Without<CursorItemBox>),
    >,
    mut cursor_item_query: Query<(
        &mut UiImage,
        &CursorItemBox,
        &mut BackgroundColor,
        &Children,
    )>,
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

    for (mut image, item_box, mut color, children) in item_box_query.iter_mut() {
        update_image(&mut image, &item_box.item_stack, &mut color, children);
    }

    for (mut image, cursor_box, mut color, children) in cursor_item_query.iter_mut() {
        update_image(&mut image, &cursor_box.item_stack, &mut color, children);
    }
}

fn cursor_visibility(
    mut window: Query<&mut Window, With<PrimaryWindow>>,
    changed_interfaces: Query<
        (&NodeConfig, &Visibility),
        (Changed<Visibility>, With<InterfaceRoot>),
    >,
) {
    for (interface_config, visibility) in changed_interfaces.iter() {
        if interface_config.is_exclusive {
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

// TODO: Getting ahead of myself, but the idea here is to append one of these to all interfaces
// that contain item boxes. This way it can be used both for equipping items and for navigating
// the item boxes through keyboard input.

//
// All interfaces with this component will render an outline around the selected item.
// An item is always selected, and it persists on open/close of the interface.
#[derive(Component)]
pub struct SelectedItemBox(pub Entity);

fn initial_select_item_box(
    mut commands: Commands,
    item_box_section_query: Query<&ItemBoxSection>,
    added_itembox_query: Query<(Entity, &ItemBox, &Parent), Added<ItemBox>>,
) {
    for (box_entity, item_box, parent) in added_itembox_query.iter() {
        if item_box.index == 0 {
            let item_box_section = item_box_section_query.get(parent.get()).unwrap();
            if item_box_section.is_equipment {
                commands
                    .entity(parent.get())
                    .insert(SelectedItemBox(box_entity));
            }
        }
    }
}

fn keyboard_select_item_box(
    keyboard: Res<Input<KeyCode>>,
    mut item_box_section_query: Query<(
        &Children,
        &Visibility,
        &mut SelectedItemBox,
    ), With<ItemBoxSection>>,
) {
    for key in keyboard.get_just_pressed() {
        for (children, visibility, mut selected) in
            item_box_section_query.iter_mut()
        {
            if visibility == Visibility::Hidden {
                continue;
            }

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
