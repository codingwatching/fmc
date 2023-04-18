use bevy::prelude::*;

use super::BlockFunctionality;

pub struct FurnacePlugin;
impl Plugin for FurnacePlugin {
    fn build(&self, app: &mut App) {
        app //.register_block_functionality("furnace", furnace_setup)
            .add_systems(Update, furnace_update);
    }
}

#[derive(Component)]
struct FurnaceTag;

pub fn furnace_setup(commands: &mut Commands, entity: Entity, _: Vec<u8>) {
    commands.entity(entity).insert(FurnaceTag);
}

pub fn furnace_update() {}
