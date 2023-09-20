use bevy::{ecs::system::EntityCommands, prelude::*, window::WindowResized, winit::WinitWindows};

use super::{InterfaceMarker, DEFAULT_FONT_HANDLE};

const FONT_SIZE: f32 = 9.0;

pub struct WidgetPlugin;
impl Plugin for WidgetPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup).add_systems(
            Update,
            (
                edit_text_box,
                update_textbox_text.after(edit_text_box),
                focus_text_box_on_click,
                focus_text_box_on_interface_change,
                hover_button,
                //scale_font.run_if(on_event::<WindowResized>()),
                //scale_borders.run_if(on_event::<WindowResized>()),
            ),
        );
    }
}

#[derive(Resource)]
struct LogicalDisplayWidth {
    width: f32,
}

fn setup(
    mut commands: Commands,
    winit_windows: NonSend<WinitWindows>,
    windows: Query<Entity, &Window>,
) {
    let entity = windows.single();
    let id = winit_windows.entity_to_winit.get(&entity).unwrap();
    let monitor = winit_windows
        .windows
        .get(id)
        .unwrap()
        .current_monitor()
        .unwrap();
    let resolution = monitor.size().to_logical(monitor.scale_factor());
    commands.insert_resource(LogicalDisplayWidth {
        width: resolution.width,
    });
}

/// Marker struct for interface text.
#[derive(Component)]
struct InterfaceText;

/// Marker struct for interface buttons
#[derive(Component)]
struct InterfaceButton;

/// Marker struct for interface components with borders.
#[derive(Component)]
struct Border;

const BORDER_SIZE: f32 = 1.0;

pub trait ChildBuilderExt<'w, 's> {
    fn spawn_button<'a>(&'a mut self, width: f32, text: &str) -> EntityCommands<'w, 's, 'a>;
    fn spawn_textbox<'a>(&'a mut self, width: f32, text: &str) -> EntityCommands<'w, 's, 'a>;
}

impl<'w, 's> ChildBuilderExt<'w, 's> for ChildBuilder<'w, 's, '_> {
    fn spawn_button<'a>(&'a mut self, width: f32, text: &str) -> EntityCommands<'w, 's, 'a> {
        let mut entity_commands = self.spawn((
            ButtonBundle {
                background_color: Color::rgb_u8(110, 110, 110).into(),
                border_color: Color::BLACK.into(),
                style: Style {
                    aspect_ratio: Some(width / 20.0),
                    width: Val::Px(width),
                    border: UiRect::all(Val::Px(BORDER_SIZE)),
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    ..default()
                },
                ..default()
            },
            InterfaceButton,
            Border,
        ));
        entity_commands.with_children(|parent| {
            parent
                // Need to spawn a parent here because the borders mess up, expanding into the
                // parent border when their position type is Absolute.
                .spawn(NodeBundle {
                    style: Style {
                        width: Val::Percent(100.0),
                        height: Val::Percent(100.0),
                        ..default()
                    },
                    ..default()
                })
                .with_children(|parent| {
                    parent.spawn((
                        NodeBundle {
                            style: Style {
                                position_type: PositionType::Absolute,
                                width: Val::Percent(100.0),
                                height: Val::Percent(100.0),
                                border: UiRect {
                                    top: Val::Px(BORDER_SIZE),
                                    left: Val::Px(BORDER_SIZE),
                                    ..default()
                                },
                                ..default()
                            },
                            border_color: Color::rgb_u8(170, 170, 170).into(),
                            ..default()
                        },
                        Border,
                    ));
                    parent.spawn((
                        NodeBundle {
                            style: Style {
                                position_type: PositionType::Absolute,
                                width: Val::Percent(100.0),
                                height: Val::Percent(100.0),
                                border: UiRect {
                                    bottom: Val::Px(BORDER_SIZE),
                                    right: Val::Px(BORDER_SIZE),
                                    ..default()
                                },
                                ..default()
                            },
                            border_color: Color::rgba_u8(62, 62, 62, 150).into(),
                            ..default()
                        },
                        Border,
                    ));
                });
            parent.spawn((
                TextBundle {
                    text: Text::from_section(
                        text,
                        TextStyle {
                            font_size: FONT_SIZE,
                            font: DEFAULT_FONT_HANDLE.clone(),
                            color: Color::DARK_GRAY,
                            ..default()
                        },
                    ),
                    style: Style {
                        position_type: PositionType::Absolute,
                        margin: UiRect {
                            top: Val::Px(1.7),
                            left: Val::Px(2.0),
                            ..default()
                        },
                        ..default()
                    },
                    ..default()
                },
                InterfaceText,
            ));
            parent.spawn((
                TextBundle {
                    text: Text::from_section(
                        text,
                        TextStyle {
                            font_size: FONT_SIZE,
                            font: DEFAULT_FONT_HANDLE.clone(),
                            color: Color::WHITE,
                            ..default()
                        },
                    ),
                    style: Style {
                        position_type: PositionType::Absolute,
                        ..default()
                    },
                    ..default()
                },
                InterfaceText,
            ));
        });
        entity_commands
    }

    fn spawn_textbox<'a>(&'a mut self, width: f32, text: &str) -> EntityCommands<'w, 's, 'a> {
        let mut entity_commands = self.spawn((
            ButtonBundle {
                background_color: Color::BLACK.into(),
                border_color: Color::WHITE.into(),
                style: Style {
                    width: Val::Percent(width),
                    aspect_ratio: Some(width / 4.2),
                    border: UiRect::all(Val::Px(BORDER_SIZE)),
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::Center,
                    overflow: Overflow::clip(),
                    ..default()
                },
                ..default()
            },
            TextBox {
                text: text.to_owned(),
            },
            Border,
        ));

        entity_commands.with_children(move |parent| {
            parent.spawn((
                TextBundle {
                    text: Text::from_section(
                        "",
                        TextStyle {
                            font_size: FONT_SIZE,
                            font: DEFAULT_FONT_HANDLE.clone(),
                            color: Color::WHITE,
                            ..default()
                        },
                    ),
                    ..default()
                },
                TextBoxText,
                InterfaceText,
            ));
        });
        entity_commands
    }
}

fn hover_button(
    mut button_query: Query<
        (&Interaction, &mut BackgroundColor),
        (With<InterfaceButton>, Changed<Interaction>),
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
            if let Ok(prev_entity) = focused_text_box.get_single() {
                commands.entity(prev_entity).remove::<FocusedTextBox>();
            }

            commands.entity(entity).insert(FocusedTextBox);
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
        for event in chars.read() {
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

// TODO: Can be removed when: https://github.com/bevyengine/bevy/pull/9524
fn scale_font(
    resolution: Res<LogicalDisplayWidth>,
    windows: Query<&Window>,
    mut text_query: Query<&mut Transform, With<InterfaceText>>,
) {
    let window = windows.single();
    let scale = window.resolution.width() / resolution.width;
    for mut transform in text_query.iter_mut() {
        transform.scale = Vec3::splat(scale);
    }
}

// TODO: This as well as the Border component are not actually needed. Borders can be defined by
// Val::Percent and will resize fine. There is currently a bug where the rightmost inner border
// overlaps the outer border, it shows less when doing it manually.
fn scale_borders(
    resolution: Res<LogicalDisplayWidth>,
    windows: Query<&Window>,
    mut text_query: Query<&mut Style, With<Border>>,
) {
    let window = windows.single();
    let scale = window.resolution.width() / resolution.width;
    for mut style in text_query.iter_mut() {
        if style.border.left != Val::Px(0.0) {
            style.border.left = Val::Px(BORDER_SIZE * scale);
        }
        if style.border.right != Val::Px(0.0) {
            style.border.right = Val::Px(BORDER_SIZE * scale);
        }
        if style.border.top != Val::Px(0.0) {
            style.border.top = Val::Px(BORDER_SIZE * scale);
        }
        if style.border.bottom != Val::Px(0.0) {
            style.border.bottom = Val::Px(BORDER_SIZE * scale);
        }
    }
}
