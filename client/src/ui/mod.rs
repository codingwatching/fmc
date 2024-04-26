use bevy::{
    asset::load_internal_binary_asset, prelude::*, window::WindowResized, winit::WinitWindows,
};

// The ui module handles two different ui systems. The 'server' system which handles in-game ui
// sent by the server at constructed at runtime, and the 'gui' system which handles 'client' ui
// e.g. the main menu, the server list and the pause menu.

mod gui;
pub mod server;
// TODO: This module doesn't fit here, it is logically a part of the player. It needs access to the
// focused item of the server interfaces to know which item to equip though, since sending what
// item to equip from the server has too much latency. To solve I think it can just be turned on
// its head. Make the server module private, expose EquippedItem from the player module and set it
// from the server module. Move hand module to player.
mod hand;
// Common widgets used between the two ui systems.
mod widgets;

pub const DEFAULT_FONT_HANDLE: Handle<Font> = Handle::weak_from_u128(1491772431825224041);

const UI_SCALE: f32 = 4.0;

// These interfaces serve as the client gui and are separate from the in-game interfaces sent by
// the server, these can be found in the 'player' module.
pub struct UiPlugin;
impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            widgets::WidgetPlugin,
            gui::GuiPlugin,
            hand::HandPlugin,
            server::ServerInterfacesPlugin,
        ))
        .add_systems(Startup, setup)
        .add_systems(Update, scale_ui.run_if(on_event::<WindowResized>()));

        // TODO: It would be nice to overwrite bevy's default handle
        // instead, so it never has to be specified by any entity, but doing it increases compile time
        // by a lot. Maybe because it reaches into the bevy crates.
        load_internal_binary_asset!(
            app,
            DEFAULT_FONT_HANDLE,
            "../../assets/ui/font.otf",
            |bytes: &[u8], _path: String| { Font::try_from_bytes(bytes.to_vec()).unwrap() }
        );
    }
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
    commands.insert_resource(LogicalMonitorWidth {
        width: resolution.width,
    });
}

#[derive(Resource)]
struct LogicalMonitorWidth {
    width: f32,
}

// TODO: Scaling like this uses a lot of memory because of how fonts sizes are stored.
// https://github.com/bevyengine/bevy/issues/5636
// It was fixed, but then reversed. Haven't found anyone discussing it afterwards.
fn scale_ui(
    mut ui_scale: ResMut<UiScale>,
    resolution: Res<LogicalMonitorWidth>,
    window: Query<&Window>,
) {
    let window = window.single();
    let scale = window.resolution.width() / resolution.width;
    ui_scale.0 = (UI_SCALE * scale) as f64;
}
