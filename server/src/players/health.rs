use bevy::prelude::*;

use fmc_networking::{messages, ConnectionId, NetworkData, NetworkServer};

use super::{player::Health, PlayerMarker, Players, RespawnEvent};

pub struct HealthPlugin;
impl Plugin for HealthPlugin {
    fn build(&self, app: &mut App) {
        app.add_event::<DamageEvent>()
            .add_event::<HealEvent>()
            .add_systems(Update, (fall_damage, heal_on_respawn).before(change_health))
            .add_systems(
                Update,
                (add_fall_damage_component, change_health, death_interface),
            );
    }
}

#[derive(Component)]
pub struct FallDamage(u32);

#[derive(Event)]
struct DamageEvent {
    entity: Entity,
    damage: u32,
}

#[derive(Event)]
struct HealEvent {
    entity: Entity,
    healing: u32,
}

fn add_fall_damage_component(
    mut commands: Commands,
    new_player_query: Query<Entity, Added<PlayerMarker>>,
) {
    for entity in new_player_query.iter() {
        commands.entity(entity).insert(FallDamage(0));
    }
}

fn fall_damage(
    players: Res<Players>,
    mut fall_damage_query: Query<(Entity, &mut FallDamage), With<PlayerMarker>>,
    mut position_events: EventReader<NetworkData<messages::PlayerPosition>>,
    mut damage_events: EventWriter<DamageEvent>,
) {
    for position_update in position_events.read() {
        let player_entity = players.get(&position_update.source);
        let (entity, mut fall_damage) = fall_damage_query.get_mut(player_entity).unwrap();

        if fall_damage.0 != 0 && position_update.velocity.y > -0.1 {
            damage_events.send(DamageEvent {
                entity,
                damage: fall_damage.0,
            });
            fall_damage.0 = 0;
        } else if position_update.velocity.y < 0.0 {
            fall_damage.0 = (position_update.velocity.y.abs() as u32).saturating_sub(15);
        }
    }
}

fn change_health(
    net: Res<NetworkServer>,
    mut health_query: Query<(&mut Health, &ConnectionId)>,
    mut damage_events: EventReader<DamageEvent>,
    mut heal_events: EventReader<HealEvent>,
) {
    for damage_event in damage_events.read() {
        let (mut health, connection_id) = health_query.get_mut(damage_event.entity).unwrap();
        let interface_update = health.take_damage(damage_event.damage);
        net.send_one(*connection_id, interface_update);

        if health.hearts == 0 {
            net.send_one(
                *connection_id,
                messages::InterfaceOpen {
                    interface_path: "death_screen".to_owned(),
                },
            );
        }
    }

    for event in heal_events.read() {
        let (mut health, connection_id) = health_query.get_mut(event.entity).unwrap();
        let interface_update = health.heal(event.healing);
        net.send_one(*connection_id, interface_update);
    }
}

fn heal_on_respawn(
    mut respawn_events: EventReader<RespawnEvent>,
    mut heal_events: EventWriter<HealEvent>,
) {
    for event in respawn_events.read() {
        heal_events.send(HealEvent {
            entity: event.entity,
            healing: u32::MAX,
        });
    }
}

fn death_interface(
    net: Res<NetworkServer>,
    players: Res<Players>,
    health_query: Query<&Health>,
    mut respawn_button_events: EventReader<NetworkData<messages::InterfaceButtonPress>>,
    mut respawn_events: EventWriter<RespawnEvent>,
) {
    for button_press in respawn_button_events.read() {
        if &button_press.interface_path != "death_screen/respawn_button" {
            return;
        }

        let entity = players.get(&button_press.source);
        let health = health_query.get(entity).unwrap();

        if health.hearts == 0 {
            respawn_events.send(RespawnEvent { entity });
            net.send_one(
                button_press.source,
                messages::InterfaceClose {
                    interface_path: "death_screen".to_owned(),
                },
            );
        }
    }
}
