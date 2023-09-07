use bevy::{
    ecs::{query::Has, system::EntityCommands},
    prelude::*,
};

use super::{InterfaceMarker, DEFAULT_FONT_HANDLE};

pub struct WidgetPlugin;
impl Plugin for WidgetPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                edit_text_box,
                update_textbox_text.after(edit_text_box),
                focus_text_box_on_click,
                focus_text_box_on_interface_change,
                hover_button,
            ),
        );
    }
}

// XXX: This conflicts with bevy's button, but works as a namespace override within the file.
#[derive(Component)]
struct Button;

pub trait ChildBuilderExt<'w, 's> {
    fn spawn_button<'a>(&'a mut self, width: f32, text: &str) -> EntityCommands<'w, 's, 'a>;
    fn spawn_textbox<'a>(
        &'a mut self,
        width: f32,
        text: &str,
    ) -> EntityCommands<'w, 's, 'a>;
}

impl<'w, 's> ChildBuilderExt<'w, 's> for ChildBuilder<'w, 's, '_> {
    fn spawn_button<'a>(&'a mut self, width: f32, text: &str) -> EntityCommands<'w, 's, 'a> {
        let mut entity_commands = self.spawn((
            ButtonBundle {
                background_color: Color::rgb_u8(110, 110, 110).into(),
                border_color: Color::BLACK.into(),
                style: Style {
                    //margin: UiRect::top(Val::Px(2.5)),
                    aspect_ratio: Some(41.5 / 4.1),
                    width: Val::Percent(width),
                    //height: Val::Percent(7.0),
                    border: UiRect::all(Val::Px(1.0)),
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    ..default()
                },
                ..default()
            },
            Button,
        ));
        entity_commands.with_children(|parent| {
            parent
                .spawn(NodeBundle {
                    style: Style {
                        width: Val::Percent(100.0),
                        height: Val::Percent(100.0),
                        ..default()
                    },
                    ..default()
                })
                .with_children(|parent| {
                    parent.spawn(NodeBundle {
                        style: Style {
                            position_type: PositionType::Absolute,
                            width: Val::Percent(100.0),
                            height: Val::Percent(100.0),
                            border: UiRect {
                                top: Val::Px(0.8),
                                left: Val::Px(0.8),
                                ..default()
                            },
                            ..default()
                        },
                        border_color: Color::rgb_u8(170, 170, 170).into(),
                        ..default()
                    });
                    parent.spawn(NodeBundle {
                        style: Style {
                            position_type: PositionType::Absolute,
                            width: Val::Percent(100.0),
                            height: Val::Percent(100.0),
                            border: UiRect {
                                bottom: Val::Px(0.8),
                                right: Val::Px(0.8),
                                ..default()
                            },
                            ..default()
                        },
                        border_color: Color::rgba_u8(62, 62, 62, 150).into(),
                        ..default()
                    });
                });
            parent.spawn(TextBundle {
                text: Text::from_section(
                    text,
                    TextStyle {
                        font_size: 9.0,
                        font: DEFAULT_FONT_HANDLE.typed(),
                        color: Color::WHITE,
                        ..default()
                    },
                ),
                style: Style {
                    position_type: PositionType::Absolute,
                    align_self: AlignSelf::Center,
                    //margin: UiRect::all(Val::Px(1.0)),
                    ..default()
                },
                ..default()
            });
        });
        entity_commands
    }

    fn spawn_textbox<'a>(
        &'a mut self,
        width: f32,
        text: &str,
    ) -> EntityCommands<'w, 's, 'a> {
        let mut entity_commands = self.spawn((
            ButtonBundle {
                background_color: Color::BLACK.into(),
                border_color: Color::WHITE.into(),
                style: Style {
                    width: Val::Percent(width),
                    aspect_ratio: Some(6.0),
                    border: UiRect::all(Val::Px(1.0)),
                    align_items: AlignItems::Center,
                    margin: UiRect::all(Val::Px(1.0)),
                    overflow: Overflow::clip(),
                    ..default()
                },
                ..default()
            },
            TextBox {
                text: text.to_owned()
            },
        ));

        entity_commands.with_children(move |parent| {
            parent.spawn((
                TextBundle {
                    text: Text::from_section(
                        "",
                        TextStyle {
                            font_size: 6.0,
                            font: DEFAULT_FONT_HANDLE.typed(),
                            color: Color::WHITE,
                            ..default()
                        },
                    ),
                    style: Style {
                        margin: UiRect::all(Val::Px(4.0)),
                        ..default()
                    },
                    ..default()
                },
                TextBoxText,
            ));
        });
        entity_commands
    }
}

fn hover_button(
    mut button_query: Query<
        (&Interaction, &mut BackgroundColor),
        (With<Button>, Changed<Interaction>),
    >,
) {
    for (interaction, mut background_color) in button_query.iter_mut() {
        if *interaction == Interaction::Hovered {
            *background_color = Color::rgb_u8(139, 139, 139).into();
        } else {
            *background_color = Color::rgb_u8(110, 110, 110).into();
        }
    }
}

#[derive(Component)]
struct FocusedTextBox;

// TODO: Needs horizontal scroll and cursor
//
/// Marker component for the textbox
#[derive(Component)]
pub struct TextBox {
    // The entire content of the textbox. The visible text might be a subset of this.
    pub text: String,
}

/// Marker component for the text inside the textbox
#[derive(Component)]
pub struct TextBoxText;

fn focus_text_box_on_click(
    mut commands: Commands,
    focused_text_box: Query<Entity, With<FocusedTextBox>>,
    possible_new_focus: Query<(Entity, &Interaction), (With<TextBox>, Changed<Interaction>)>,
) {
    for (entity, interaction) in possible_new_focus.iter() {
        if *interaction == Interaction::Pressed {
            commands.entity(entity).insert(FocusedTextBox);

            if let Ok(prev_entity) = focused_text_box.get_single() {
                commands.entity(prev_entity).remove::<FocusedTextBox>();
            }
        }
    }
}

fn focus_text_box_on_interface_change(
    mut commands: Commands,
    focused_text_box: Query<Entity, With<FocusedTextBox>>,
    text_box_query: Query<Entity, With<TextBox>>,
    interfaces_query: Query<(&Style, &Children), (With<InterfaceMarker>, Changed<Style>)>,
) {
    for (style, children) in interfaces_query.iter() {
        if style.display == Display::Flex {
            for child_entity in children.iter() {
                if let Ok(entity) = text_box_query.get(*child_entity) {
                    commands.entity(entity).insert(FocusedTextBox);
                }
            }

            if let Ok(prev_entity) = focused_text_box.get_single() {
                commands.entity(prev_entity).remove::<FocusedTextBox>();
            }
        }
    }
}

fn edit_text_box(
    mut focused_text_box: Query<&mut TextBox, With<FocusedTextBox>>,
    mut chars: EventReader<ReceivedCharacter>,
) {
    if let Ok(mut text_box) = focused_text_box.get_single_mut() {
        // TODO: There is currently no way to read the keyboard input properly. Res<Input<Keycode>> has
        // no utility function for discerning if it is a valid char, you have to match the whole thing,
        // but more importantly is does not consider the repeat properties of the WM.
        for event in chars.iter() {
            if event.char.is_ascii() {
                if !event.char.is_control() {
                    text_box.text.push(event.char.to_ascii_lowercase());
                } else if event.char == '\u{8}' {
                    // This is backspace (pray)
                    text_box.text.pop();
                }
            }
        }
    }
}

fn update_textbox_text(
    mut text_query: Query<&mut Text, With<TextBoxText>>,
    text_box_query: Query<(&TextBox, &Children), Changed<TextBox>>,
) {
    for (text_box, children) in text_box_query.iter() {
        let mut text = text_query.get_mut(children[0]).unwrap();
        text.sections[0].value = text_box.text.clone();
    }
}
