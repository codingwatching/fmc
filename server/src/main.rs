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

fn main() {
    // TODO: Some resources are inserted at app build, and the rest in the startup schedules. What
    // depends on what is completely opaque. It would be nice to have it be explicit, but I don't
    // want to dirty the namespaces with loading functions to congregate them all in the same spot.
    // Maybe it's possible with systemsets, but I don't know how to flush commands with them.
    // Ideally I would want to just cram everything into Startup and mark each loading function
    // with a .run_if(this_or_that_resource.exists()) and have them magically ordered by bevy.
    App::new()
        .insert_resource(ScheduleRunnerSettings {
            run_mode: RunMode::Loop {
                // Run at ~60 ticks a second
                wait: Some(std::time::Duration::from_millis(16)),
            },
        })
        .add_plugins(MinimalPlugins)
        .add_plugin(bevy::hierarchy::HierarchyPlugin::default())
        .add_plugin(bevy_extensions::f64_transform::TransformPlugin)
        .add_plugin(bevy::log::LogPlugin::default())
        //.add_plugin(bevy::diagnostic::DiagnosticsPlugin::default())
        //.add_plugin(LogDiagnosticsPlugin::default())
        //.add_plugin(FrameTimeDiagnosticsPlugin::default())
        // Server specific
        .insert_resource(settings::ServerSettings::load())
        .add_plugin(assets::AssetPlugin)
        .add_plugin(database::DatabasePlugin)
        .add_plugin(networking::ServerPlugin)
        .add_plugin(world::WorldPlugin)
        .add_plugin(physics::PhysicsPlugin)
        .add_plugin(players::PlayersPlugin)
        .run();
}
