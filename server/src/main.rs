use bevy::{app::ScheduleRunnerPlugin, prelude::*};

mod api;
mod assets;
mod bevy_extensions;
mod chat;
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
        // Run at ~60 ticks a second
        .add_plugins(ScheduleRunnerPlugin::run_loop(
            std::time::Duration::from_millis(16),
        ))
        .add_plugins(bevy::core::TaskPoolPlugin::default())
        //.add_plugins(bevy::core::TypeRegistrationPlugin::default())
        .add_plugins(bevy::time::TimePlugin::default())
        .add_plugins(bevy::hierarchy::HierarchyPlugin::default())
        .add_plugins(bevy::log::LogPlugin::default())
        .add_plugins(bevy_extensions::f64_transform::TransformPlugin)
        //.add_plugin(bevy::diagnostic::DiagnosticsPlugin::default())
        //.add_plugin(LogDiagnosticsPlugin::default())
        //.add_plugin(FrameTimeDiagnosticsPlugin::default())
        // Server specific
        .insert_resource(settings::Settings::load())
        .add_plugins(assets::AssetPlugin)
        .add_plugins(database::DatabasePlugin)
        .add_plugins(networking::ServerPlugin)
        .add_plugins(world::WorldPlugin)
        .add_plugins(physics::PhysicsPlugin)
        .add_plugins(players::PlayersPlugin)
        .add_plugins(chat::ChatPlugin)
        .run();
}
