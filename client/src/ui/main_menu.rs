use bevy::prelude::*;
use fmc_networking::NetworkSettings;

use crate::game_state::GameState;

use super::{text::TextBoxBundle, Interfaces, UiState};

pub(super) struct MainMenuPlugin;
impl Plugin for MainMenuPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup).add_systems(
            Update,
            press_play_button.run_if(in_state(UiState::MainMenu)),
        );
    }
}

#[derive(Component)]
struct PlayButtonMarker;

#[derive(Component)]
struct ServerIpMarker;

fn setup(
    mut commands: Commands,
    mut interfaces: ResMut<Interfaces>,
    asset_server: Res<AssetServer>,
) {
    let entity = commands
        .spawn(NodeBundle {
            background_color: Color::BLACK.into(),
            style: Style {
                flex_direction: FlexDirection::Column,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                gap: Size {
                    width: Val::Auto,
                    height: Val::Percent(2.0),
                },
                size: Size {
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                },
                position_type: PositionType::Absolute,
                ..default()
            },
            ..default()
        })
        .with_children(|parent| {
            parent
                .spawn(NodeBundle {
                    background_color: Color::WHITE.into(),
                    style: Style {
                        size: Size {
                            width: Val::Percent(30.0),
                            height: Val::Percent(5.0),
                        },
                        overflow: Overflow::Hidden,
                        ..default()
                    },
                    ..default()
                })
                .with_children(|parent| {
                    parent
                        .spawn(TextBoxBundle {
                            background_color: Color::BLACK.into(),
                            style: Style {
                                align_items: AlignItems::Center,
                                margin: UiRect::all(Val::Px(1.0)),
                                flex_grow: 1.0,
                                ..default()
                            },
                            ..default()
                        })
                        .with_children(|parent| {
                            parent
                                .spawn(TextBundle {
                                    text: Text::from_section(
                                        "127.0.0.1",
                                        TextStyle {
                                            font: asset_server.load("assets/ui/font.otf"),
                                            font_size: 6.0,
                                            color: Color::WHITE,
                                        },
                                    ),
                                    style: Style {
                                        margin: UiRect::all(Val::Px(4.0)),
                                        ..default()
                                    },
                                    ..default()
                                })
                                .insert(ServerIpMarker);
                        });
                });

            // Play button
            parent
                .spawn(NodeBundle {
                    background_color: Color::WHITE.into(),
                    style: Style {
                        size: Size {
                            width: Val::Percent(30.0),
                            height: Val::Percent(5.0),
                        },
                        ..default()
                    },
                    ..default()
                })
                .with_children(|parent| {
                    parent
                        .spawn(ButtonBundle {
                            background_color: Color::BLACK.into(),
                            style: Style {
                                align_items: AlignItems::Center,
                                justify_content: JustifyContent::Center,
                                margin: UiRect::all(Val::Px(1.0)),
                                flex_grow: 1.0,
                                ..default()
                            },
                            ..default()
                        })
                        .with_children(|parent| {
                            parent.spawn(TextBundle {
                                text: Text::from_section(
                                    "PLAY",
                                    TextStyle {
                                        font: asset_server.load("assets/ui/font.otf"),
                                        font_size: 6.0,
                                        color: Color::WHITE,
                                    },
                                ),
                                style: Style {
                                    align_self: AlignSelf::Center,
                                    margin: UiRect::all(Val::Px(1.0)),
                                    ..default()
                                },
                                ..default()
                            });
                        })
                        .insert(PlayButtonMarker);
                });
        })
        .id();
    interfaces.insert(UiState::MainMenu, entity);
}

fn press_play_button(
    mut net: ResMut<fmc_networking::NetworkClient>,
    server_ip: Query<&Text, With<ServerIpMarker>>,
    play_button: Query<&Interaction, (Changed<Interaction>, With<PlayButtonMarker>)>,
    mut game_state: ResMut<NextState<GameState>>,
) {
    for interaction in play_button.iter() {
        if *interaction == Interaction::Clicked {
            let mut ip = server_ip.single().sections[0].value.to_owned();

            if !ip.contains(":") {
                ip.push_str(":42069");
            }

            net.connect(ip.clone(), NetworkSettings::default());
            game_state.set(GameState::Connecting);
        }
    }
}
