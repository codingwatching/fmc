use bevy::{
    app::{RunMode, ScheduleRunnerSettings},
    prelude::*,
};

mod api;
mod assets;
mod bevy_extensions;
mod constants;
mod database;
mod networking;
mod physics;
mod players;
mod settings;
mod utils;
mod world;

// TODO: Sepeprate out setup into functions. Currently if stuff fails the errors mix with the bevy
// errors and it's hard to parse for the human.
fn main() {
    // TODO: The plugins are ordered, some plugins rely on others. Which are relied on is
    // opaque. I want a way to make the dependecy tree explicit.
    App::new()
        .insert_resource(ScheduleRunnerSettings {
            run_mode: RunMode::Loop {
                // Run at ~60 ticks a second
                wait: Some(std::time::Duration::from_millis(16)),
            },
        })
        // Bevy specific
        .add_plugins(MinimalPlugins)
        .add_plugin(bevy::hierarchy::HierarchyPlugin::default())
        .add_plugin(bevy_extensions::f64_transform::TransformPlugin)
        .add_plugin(bevy::log::LogPlugin::default())
        //.add_plugin(bevy::diagnostic::DiagnosticsPlugin::default())
        //.add_plugin(LogDiagnosticsPlugin::default())
        //.add_plugin(FrameTimeDiagnosticsPlugin::default())
        // Server specific
        .insert_resource(settings::ServerSettings::read())
        .add_plugin(assets::AssetPlugin)
        .add_plugin(database::DatabasePlugin)
        .add_plugin(networking::ServerPlugin)
        .add_plugin(world::WorldPlugin)
        .add_plugin(physics::PhysicsPlugin)
        .add_plugin(players::PlayersPlugin)
        .run();
}
