use bevy::prelude::*;

use super::{InterfaceBundle, Interfaces, UiState};
use crate::{game_state::GameState, ui::widgets::*};

pub struct PauseMenuPlugin;
impl Plugin for PauseMenuPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup)
            .add_systems(Update, (press_resume, press_quit).run_if(in_state(UiState::PauseMenu)));
    }
}

#[derive(Component)]
struct ResumeButton;

#[derive(Component)]
struct QuitButton;

fn setup(mut commands: Commands, mut interfaces: ResMut<Interfaces>) {
    let entity = commands
        .spawn(InterfaceBundle {
            background_color: Color::DARK_GRAY.with_a(0.5).into(),
            style: Style {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                row_gap: Val::Px(4.0),
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            ..default()
        })
        .with_children(|parent| {
            parent.spawn_button(200.0, "Resume").insert(ResumeButton);
            parent.spawn_button(200.0, "Quit").insert(QuitButton);
        })
        .id();
    interfaces.insert(UiState::PauseMenu, entity);
}

fn press_quit(
    mut game_state: ResMut<NextState<GameState>>,
    button_query: Query<&Interaction, (Changed<Interaction>, With<QuitButton>)>,
) {
    if let Ok(interaction) = button_query.get_single() {
        if *interaction == Interaction::Pressed {
            game_state.set(GameState::MainMenu);
        }
    }
}

fn press_resume(
    mut game_state: ResMut<NextState<GameState>>,
    button_query: Query<&Interaction, (Changed<Interaction>, With<ResumeButton>)>,
) {
    if let Ok(interaction) = button_query.get_single() {
        if *interaction == Interaction::Pressed {
            game_state.set(GameState::Playing);
        }
    }
}
