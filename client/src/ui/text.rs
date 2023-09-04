use bevy::{prelude::*, ui::FocusPolicy};

pub(super) struct TextPlugin;
impl Plugin for TextPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (edit_text_box, focus_text_box));
    }
}

#[derive(Component)]
struct FocusedText;

#[derive(Component, Debug, Clone)]
pub(super) struct TextBoxMarker;

// TODO: This leaves something to be desired, you still have to add the background and textbundle
// manually.
#[derive(Bundle, Clone, Debug)]
pub(super) struct TextBoxBundle {
    pub text_box: TextBoxMarker,
    /// Describes the logical size of the node
    pub node: Node,
    /// Describes the style including flexbox settings
    pub style: Style,
    /// Describes whether and how the button has been interacted with by the input
    pub interaction: Interaction,
    /// Whether this node should block interaction with lower nodes
    pub focus_policy: FocusPolicy,
    /// The background color, which serves as a "fill" for this node
    ///
    /// When combined with `UiImage`, tints the provided image.
    pub background_color: BackgroundColor,
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
}

impl Default for TextBoxBundle {
    fn default() -> Self {
        Self {
            text_box: TextBoxMarker,
            focus_policy: FocusPolicy::Block,
            node: Default::default(),
            style: Default::default(),
            interaction: Default::default(),
            background_color: Default::default(),
            transform: Default::default(),
            global_transform: Default::default(),
            visibility: Default::default(),
            computed_visibility: Default::default(),
            z_index: Default::default(),
        }
    }
}

fn focus_text_box(
    mut commands: Commands,
    focused_text_box: Query<Entity, With<FocusedText>>,
    possible_new_focus: Query<
        (&Interaction, Option<&TextBoxMarker>, Option<&Children>),
        Changed<Interaction>,
    >,
) {
    for (interaction, text_box_marker, children) in possible_new_focus.iter() {
        if *interaction == Interaction::Pressed {
            if let Ok(prev_entity) = focused_text_box.get_single() {
                commands.entity(prev_entity).remove::<FocusedText>();
            }

            if text_box_marker.is_some() {
                commands.entity(children.unwrap()[0]).insert(FocusedText);
            }
        }
    }
}

fn edit_text_box(
    mut focused_text_box: Query<&mut Text, With<FocusedText>>,
    mut chars: EventReader<ReceivedCharacter>,
) {
    let Ok(mut text) = focused_text_box.get_single_mut() else {
        return;
    };

    // TODO: There is currently no way to read the keyboard input properly. Res<Input<Keycode>> has
    // no utility function for discerning if it is a valid char, you have to match the whole thing,
    // but more importantly is does not consider the repeat properties of the WM.
    for event in chars.iter() {
        if event.char.is_ascii() {
            if !event.char.is_control() {
                text.sections[0].value.push(event.char.to_ascii_lowercase());
            } else if event.char == '\u{8}' {
                // This is backspace (pray)
                text.sections[0].value.pop();
            }
        }
    }
}
