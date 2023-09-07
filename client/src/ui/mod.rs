use std::collections::HashMap;

use bevy::{
    asset::load_internal_binary_asset,
    prelude::*,
    reflect::TypeUuid,
    ui::FocusPolicy,
    window::{CursorGrabMode, PrimaryWindow},
};

use crate::game_state::GameState;

mod main_menu;
mod multiplayer;
mod widgets;

const DEFAULT_FONT_HANDLE: HandleUntyped =
    HandleUntyped::weak_from_u64(Font::TYPE_UUID, 1491772431825224041);

// These interfaces serve as the client gui and are separate from the in-game interfaces sent by
// the server, these can be found in the 'player' module.
pub struct UiPlugin;
impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_state::<UiState>()
            .insert_resource(Interfaces::default());

        app.add_plugins(main_menu::MainMenuPlugin)
            .add_plugins(multiplayer::MultiPlayerPlugin)
            .add_plugins(widgets::WidgetPlugin)
            .add_systems(Startup, player_cursor_setup)
            .add_systems(Update, change_interface.run_if(state_changed::<UiState>()))
            .add_systems(OnExit(GameState::MainMenu), enter_exit_ui)
            .add_systems(
                OnEnter(GameState::MainMenu),
                (enter_exit_ui, release_cursor),
            );

        // TODO: It would be nice to overwrite bevy's DEFAULT_FONT_HANDLE instead, so it never has
        // to be specified by any entity. Doing it increases compile time by a lot because it
        // reaches into the bevy crate I think.
        load_internal_binary_asset!(
            app,
            DEFAULT_FONT_HANDLE,
            "../../assets/ui/font.otf",
            |bytes: &[u8], _path: String| { Font::try_from_bytes(bytes.to_vec()).unwrap() }
        );
    }
}

// TODO: Make sub states(https://github.com/bevyengine/bevy/issues/8187)
// of the main GameState?
#[derive(States, PartialEq, Eq, Debug, Clone, Hash, Default)]
enum UiState {
    #[default]
    None,
    MainMenu,
    MultiPlayer,
}

#[derive(Resource, Deref, DerefMut, Default)]
struct Interfaces(HashMap<UiState, Entity>);

#[derive(Component)]
struct InterfaceMarker;

#[derive(Bundle)]
struct InterfaceBundle {
    /// Describes the logical size of the node
    pub node: Node,
    /// Styles which control the layout (size and position) of the node and it's children
    /// In some cases these styles also affect how the node drawn/painted.
    pub style: Style,
    /// The background color, which serves as a "fill" for this node
    pub background_color: BackgroundColor,
    /// The color of the Node's border
    pub border_color: BorderColor,
    /// Whether this node should block interaction with lower nodes
    pub focus_policy: FocusPolicy,
    /// The transform of the node
    ///
    /// This field is automatically managed by the UI layout system.
    /// To alter the position of the `NodeBundle`, use the properties of the [`Style`] component.
    pub transform: Transform,
    /// The global transform of the node
    ///
    /// This field is automatically managed by the UI layout system.
    /// To alter the position of the `NodeBundle`, use the properties of the [`Style`] component.
    pub global_transform: GlobalTransform,
    /// Describes the visibility properties of the node
    pub visibility: Visibility,
    /// Algorithmically-computed indication of whether an entity is visible and should be extracted for rendering
    pub computed_visibility: ComputedVisibility,
    /// Indicates the depth at which the node should appear in the UI
    pub z_index: ZIndex,
    /// Marker for interfaces
    interface_marker: InterfaceMarker,
}

impl Default for InterfaceBundle {
    fn default() -> Self {
        InterfaceBundle {
            // Transparent background
            background_color: Color::NONE.into(),
            border_color: Color::NONE.into(),
            node: Default::default(),
            style: Default::default(),
            focus_policy: Default::default(),
            transform: Default::default(),
            global_transform: Default::default(),
            visibility: Default::default(),
            computed_visibility: Default::default(),
            z_index: Default::default(),
            interface_marker: InterfaceMarker,
        }
    }
}

fn change_interface(
    state: Res<State<UiState>>,
    interfaces: Res<Interfaces>,
    mut interface_query: Query<(Entity, &mut Style), With<InterfaceMarker>>,
) {
    let new_interface_entity = interfaces.get(state.get());
    for (interface_entity, mut style) in interface_query.iter_mut() {
        if new_interface_entity.is_some() && *new_interface_entity.unwrap() == interface_entity {
            style.display = Display::Flex;
        } else {
            style.display = Display::None;
        }
    }
}

fn enter_exit_ui(game_state: Res<State<GameState>>, mut ui_state: ResMut<NextState<UiState>>) {
    if *game_state == GameState::MainMenu {
        ui_state.set(UiState::MainMenu);
    } else {
        ui_state.set(UiState::None);
    }
}

// The cross placed in the middle while playing
// TODO: Make it a cross instead of a dot, and white transparent
fn player_cursor_setup(mut commands: Commands, mut interfaces: ResMut<Interfaces>) {
    // red dot cursor
    let entity = commands
        .spawn(NodeBundle {
            style: Style {
                width: Val::Px(3.0),
                height: Val::Px(3.0),
                position_type: PositionType::Absolute,
                left: Val::Percent(50.0),
                bottom: Val::Percent(50.0),
                ..default()
            },
            background_color: BackgroundColor(Color::rgba(0.9, 0.9, 0.9, 0.3)),
            ..Default::default()
        })
        .id();

    interfaces.insert(UiState::None, entity);
}

fn release_cursor(mut window: Query<&mut Window, With<PrimaryWindow>>) {
    let mut window = window.single_mut();
    window.cursor.grab_mode = CursorGrabMode::None;
    window.cursor.visible = true;
}
