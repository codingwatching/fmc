use bevy::prelude::*;
use fmc_networking::{messages, NetworkServer};

pub struct SkyPlugin;
impl Plugin for SkyPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, day_night_cycle);
    }
}

const DAY_LENGTH: f32 = 1200.0;

// time == 0, dawn
// time == 600, dusk
fn day_night_cycle(bevy_time: Res<Time>, net: Res<NetworkServer>, mut time: Local<f32>) {
    *time += bevy_time.delta_seconds();
    *time %= DAY_LENGTH;

    let message = messages::Time {
        angle: *time * std::f32::consts::TAU / DAY_LENGTH,
    };
    net.broadcast(message);
}
