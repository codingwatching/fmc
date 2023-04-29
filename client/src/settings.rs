// TODO: Settings should be loaded from xdg_config dir, default stored in etc
use bevy::prelude::Resource;

#[derive(Resource)]
pub struct Settings {
    // Render distance in chunks
    pub render_distance: u32,
    pub volume: i32,
    pub sensitivity: f32,
    pub flight_speed: f32,
}

impl Settings {
    /// Loads the configuration file
    //fn load() {
    //
    //}

    // temporary default values
    pub fn new() -> Self {
        Self {
            render_distance: 15,
            volume: 100,
            sensitivity: 0.00012,
            flight_speed: 50.,
        }
    }
}
