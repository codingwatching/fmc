use bevy::{
    app::ScheduleRunnerPlugin,
    //diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin},
    prelude::*,
};

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
    // TODO: It might make sense to increase the amount of cpu threads used by the async compute pool
    // since most of the work done is to produce chunks.
    //
    // TODO: Some resources are inserted at app build, and the rest in the startup schedules. What
    // depends on what is completely opaque. It would be nice to have it be explicit, but I don't
    // want to dirty the namespaces with loading functions to congregate them all in the same spot.
    // Maybe it's possible with systemsets, but I don't know how to flush commands with them.
    // Ideally I would want to just cram everything into Startup and mark each loading function
    // with a .run_if(this_or_that_resource.exists()) and have them magically ordered by bevy.
    // Development: I think this is possible to do with systemsets now. Looks like it does
    // apply_deferred when it's necessary if the sets are chained.
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
        //.add_plugins(bevy::diagnostic::DiagnosticsPlugin::default())
        //.add_plugins(LogDiagnosticsPlugin::default())
        //.add_plugins(FrameTimeDiagnosticsPlugin::default())
        .add_plugins(FrameCountPlugin::default())
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
