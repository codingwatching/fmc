use bevy::prelude::*;

use crate::ui::InterfaceBundle;

use super::{widgets::*, Interfaces, UiState};

pub(super) struct MainMenuPlugin;
impl Plugin for MainMenuPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup).add_systems(
            Update,
            press_multiplayer.run_if(in_state(UiState::MainMenu)),
        );
    }
}

#[derive(Component)]
struct SinglePlayerButton;
#[derive(Component)]
struct MultiPlayerButton;

fn setup(mut commands: Commands, mut interfaces: ResMut<Interfaces>) {
    let entity = commands
        .spawn(InterfaceBundle {
            background_color: Color::DARK_GRAY.into(),
            style: Style {
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                row_gap: Val::Px(4.0),
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                position_type: PositionType::Absolute,
                ..default()
            },
            ..default()
        })
        .with_children(|parent| {
            // Singleplayer button
            parent
                .spawn_button(200.0, "Singleplayer (Not implemented)")
                .insert(SinglePlayerButton);
            parent
                .spawn_button(200.0, "Multiplayer")
                .insert(MultiPlayerButton);
        })
        .id();
    interfaces.insert(UiState::MainMenu, entity);
}

//fn press_singleplayer(
//    mut ui_state: ResMut<NextState<UiState>>,
//    button_query: Query<&Interaction, (Changed<Interaction>, With<SinglePlayerButton>)>,
//) {
//    if let Ok(interaction) = button_query.get_single() {
//        if *interaction == Interaction::Pressed {
//            ui_state.set(UiState::MultiPlayer);
//        }
//    }
//}

fn press_multiplayer(
    mut ui_state: ResMut<NextState<UiState>>,
    button_query: Query<&Interaction, (Changed<Interaction>, With<MultiPlayerButton>)>,
) {
    if let Ok(interaction) = button_query.get_single() {
        if *interaction == Interaction::Pressed {
            ui_state.set(UiState::MultiPlayer);
        }
    }
}
