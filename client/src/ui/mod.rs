use std::collections::HashMap;

use bevy::{
    prelude::*,
    window::{CursorGrabMode, PrimaryWindow},
};

use crate::game_state::GameState;

mod main_menu;
mod text;

// These interfaces serve as the client gui and are separate from the in-game interfaces sent by
// the server, these can be found in the 'player' directory.
pub struct UiPlugin;
impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_state::<UiState>()
            .insert_resource(Interfaces::default());

        app.add_plugin(main_menu::MainMenuPlugin)
            .add_plugin(text::TextPlugin)
            .add_systems(Startup, player_cursor_setup)
            .add_systems(Update, change_interface.run_if(state_changed::<UiState>()))
            .add_systems(OnExit(GameState::MainMenu), enter_exit_ui)
            .add_systems(
                OnEnter(GameState::MainMenu),
                (enter_exit_ui, release_cursor),
            );
    }
}

// TODO: Make sub states(https://github.com/bevyengine/bevy/issues/8187)
// of the main GameState?
#[derive(States, PartialEq, Eq, Debug, Clone, Hash, Default)]
enum UiState {
    None,
    #[default]
    MainMenu,
}

#[derive(Resource, Deref, DerefMut, Default)]
struct Interfaces(HashMap<UiState, Entity>);

fn change_interface(
    state: Res<State<UiState>>,
    interfaces: Res<Interfaces>,
    // TODO: Maybe this should be made explicit by a UiRoot component. It's probably going to pick
    // up something that wasn't intended.
    mut interface_query: Query<(Entity, &mut Style), (With<Node>, Without<Parent>)>,
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
                size: Size::new(Val::Px(3.0), Val::Px(3.0)),
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
