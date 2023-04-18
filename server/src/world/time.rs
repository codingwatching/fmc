use bevy::prelude::*;
use fmc_networking::{messages, NetworkServer};

/// Keeps track of the passage of time.
pub struct TimePlugin;
impl Plugin for TimePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, day_night_cycle);
    }
}
