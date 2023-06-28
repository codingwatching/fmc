use bevy::prelude::*;

use fmc_networking::{messages, ConnectionId, NetworkData, NetworkServer};

use crate::{
    players::Players,
    world::items::{
        crafting::{RecipeCollection, Recipes},
        ItemStack, ItemStorage, Items,
    },
};

use super::{PlayerEquipment, PlayerEquippedItem, PlayerInventoryCraftingTable, PlayerName};

pub struct InventoryPlugin;
impl Plugin for InventoryPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                equip_item,
                insert_held_item_component,
                update_inventory_interface,
                send_initial_inventory_interface,
            ),
        );
    }
}

/// The output of interface actions.
#[derive(Default)]
struct PlayerInterfaceUpdate {
    pub hotbar: Option<messages::InterfaceItemBoxUpdate>,
    pub inventory: Option<messages::InterfaceItemBoxUpdate>,
}

// Items that are taken from the interface are stored in this until they are placed again. No new
// items are allowed to be taken until it has been placed.
#[derive(Component, Deref, DerefMut)]
struct HeldItemStack(ItemStack);

// Takes care of both the hotbar interface and the inventory interface as the hotbar shares items
// with the inventory.
struct PlayerInventoryInterface<'a> {
    inventory: &'a mut ItemStorage,
    equipment: &'a mut PlayerEquipment,
    crafting_table: &'a mut PlayerInventoryCraftingTable,
    recipes: &'a RecipeCollection,
    item_configs: &'a Items,
}

impl PlayerInventoryInterface<'_> {
    fn build(&self) -> PlayerInterfaceUpdate {
        let mut inventory = self.build_inventory();
        inventory.combine(self.build_crafting_table());

        let hotbar = self.build_hotbar();

        return PlayerInterfaceUpdate {
            hotbar: Some(hotbar),
            inventory: Some(inventory),
        };
    }

    fn build_hotbar(&self) -> messages::InterfaceItemBoxUpdate {
        let mut hotbar = messages::InterfaceItemBoxUpdate::new("hotbar", false);

        for (i, item_stack) in self.inventory[0..9].iter().enumerate() {
            if let Some(item) = item_stack.item() {
                hotbar.add_itembox(
                    0,
                    i as u32,
                    item.id,
                    item_stack.size,
                    item.properties["durability"].as_u32(),
                    item.properties["description"].as_str(),
                );
            } else {
                hotbar.add_empty_itembox(0, i as u32);
            }
        }

        return hotbar;
    }

    fn build_inventory(&self) -> messages::InterfaceItemBoxUpdate {
        let mut inventory = messages::InterfaceItemBoxUpdate::new("inventory", false);

        // hotbar section
        for (i, item_stack) in self.inventory[0..9].iter().enumerate() {
            if let Some(item) = item_stack.item() {
                inventory.add_itembox(
                    0,
                    i as u32,
                    item.id,
                    item_stack.size,
                    item.properties["durability"].as_u32(),
                    item.properties["description"].as_str(),
                );
            } else {
                inventory.add_empty_itembox(0, i as u32);
            }
        }

        // main inventory section
        for (i, item_stack) in self.inventory[9..36].iter().enumerate() {
            // TODO: There's some kinda bug when in the layout when you stretch two or more lines
            // more than 124 pixels(should be 160 here). So there's 6 missing items slots at the
            // moment...
            if i == 21 {
                break;
            }
            if let Some(item) = item_stack.item() {
                inventory.add_itembox(
                    1,
                    i as u32,
                    item.id,
                    item_stack.size,
                    item.properties["durability"].as_u32(),
                    item.properties["description"].as_str(),
                );
            } else {
                inventory.add_empty_itembox(1, i as u32);
                //inventory.add_itembox(
                //    1, i as u32, 1, 2, None,
                //    None,
                //);
            }
        }

        // equipment section
        for (i, item_stack) in self.equipment.iter().enumerate() {
            if let Some(item) = item_stack.item() {
                inventory.add_itembox(
                    i as u32 + 2,
                    0,
                    item.id,
                    item_stack.size,
                    item.properties["durability"].as_u32(),
                    item.properties["description"].as_str(),
                );
            } else {
                inventory.add_empty_itembox(i as u32 + 2, 0);
            }
        }

        return inventory;
    }

    fn build_crafting_table(&self) -> messages::InterfaceItemBoxUpdate {
        let mut crafting_table = messages::InterfaceItemBoxUpdate::new("inventory", false);

        for (i, item_stack) in self.crafting_table.iter().enumerate() {
            if let Some(item) = item_stack.item() {
                crafting_table.add_itembox(
                    6,
                    i as u32,
                    item.id,
                    item_stack.size,
                    item.properties["durability"].as_u32(),
                    item.properties["description"].as_str(),
                );
            } else {
                crafting_table.add_empty_itembox(6, i as u32);
            }
        }

        if let Some((item, amount)) = self.recipes.get_output(&self.crafting_table.as_slice()) {
            crafting_table.add_itembox(
                7,
                0,
                item.id,
                amount,
                item.properties["durability"].as_u32(),
                item.properties["description"].as_str(),
            );
        } else {
            crafting_table.add_empty_itembox(7, 0 as u32);
        }

        return crafting_table;
    }

    // Take items out of a stack through the interface, if the index doesn't match the amount, it
    // returns None.
    fn take_item(
        &mut self,
        section: u32,
        index: u32,
        amount: u32,
        held_item_stack: &mut ItemStack,
    ) -> PlayerInterfaceUpdate {
        let mut interface_update = PlayerInterfaceUpdate::default();

        match section {
            0 => {
                let item_stack = &mut self.inventory[index as usize];

                held_item_stack.transfer(item_stack, amount);

                // Update hotbar since inventory actions affect it.
                let mut hotbar_update = messages::InterfaceItemBoxUpdate::new("hotbar", false);
                if item_stack.is_empty() {
                    hotbar_update.add_empty_itembox(0, index);
                } else {
                    let item = item_stack.item().unwrap();

                    hotbar_update.add_itembox(
                        section,
                        index,
                        item.id,
                        item_stack.size,
                        item.properties["durability"].as_u32(),
                        item.properties["description"].as_str(),
                    );
                };

                interface_update.hotbar = Some(hotbar_update);
            }
            1 => {
                let item_stack = &mut self.inventory[9 + index as usize];
                held_item_stack.transfer(item_stack, amount);
            }
            2..=5 => {
                let item_stack = &mut self.equipment[index as usize];
                held_item_stack.transfer(item_stack, amount);
            }
            6 => {
                let item_stack = &mut self.crafting_table[index as usize];
                held_item_stack.transfer(item_stack, amount);

                interface_update.inventory = Some(self.build_crafting_table());
            }
            7 => {
                if let Some(recipe) = self.recipes.get_recipe(&self.crafting_table.as_slice()) {
                    let output_item = recipe.output_item();
                    let item_config = self.item_configs.get_config(&output_item.id);

                    if held_item_stack.is_empty() || held_item_stack.item().unwrap() == output_item
                    {
                        let amount = if held_item_stack.is_empty() {
                            std::cmp::min(item_config.max_stack_size, amount)
                        } else {
                            std::cmp::min(held_item_stack.capacity(), amount)
                        };

                        if let Some((item, amount)) =
                            recipe.craft(&mut self.crafting_table.as_mut_slice(), amount)
                        {
                            // TODO: Clean up when craft return value is converted to ItemStack
                            *held_item_stack = ItemStack::new(
                                item,
                                held_item_stack.size() + amount,
                                item_config.max_stack_size,
                            );
                            interface_update.inventory = Some(self.build_crafting_table());
                        }
                    }
                }
            }
            _ => (),
        };

        return interface_update;
    }

    // Place an item stack in a slot. If the slot is occupied it replaces it and returns what was
    // there.
    fn place_item(
        &mut self,
        section: u32,
        index: u32,
        amount: u32,
        held_item_stack: &mut ItemStack,
    ) -> PlayerInterfaceUpdate {
        let mut interface_update = PlayerInterfaceUpdate::default();

        match section {
            0 => {
                let item_box_stack = &mut self.inventory[index as usize];
                item_box_stack.transfer(held_item_stack, amount);

                let mut hotbar_update = messages::InterfaceItemBoxUpdate::new("hotbar", false);
                if let Some(item) = item_box_stack.item() {
                    hotbar_update.add_itembox(
                        section,
                        index,
                        item.id,
                        item_box_stack.size(),
                        item.properties["durability"].as_u32(),
                        item.properties["description"].as_str(),
                    );
                } else {
                    hotbar_update.add_empty_itembox(section, index);
                }

                interface_update.hotbar = Some(hotbar_update);
            }
            1 => {
                let item_box_stack = &mut self.inventory[9 + index as usize];
                item_box_stack.transfer(held_item_stack, amount);
            }
            2..=5 => {
                if held_item_stack.size != 1 {
                    return interface_update;
                }

                let item = held_item_stack.item().unwrap();
                let categories = match &self.item_configs.get_config(&item.id).categories {
                    Some(c) => c,
                    None => return interface_update,
                };

                match section {
                    2 if categories.contains("helmet") => self.equipment[0].swap(held_item_stack),
                    3 if categories.contains("chestplate") => {
                        self.equipment[1].swap(held_item_stack)
                    }
                    4 if categories.contains("leggings") => self.equipment[2].swap(held_item_stack),
                    5 if categories.contains("boots") => self.equipment[3].swap(held_item_stack),
                    _ => return interface_update,
                };
            }
            6 => {
                let item_box_stack = &mut self.crafting_table[index as usize];
                item_box_stack.transfer(held_item_stack, amount);

                interface_update.inventory = Some(self.build_crafting_table());
            }
            _ => return interface_update,
        };

        return interface_update;
    }
}

// Add an `HeldItemStack` to every player
fn insert_held_item_component(
    mut commands: Commands,
    player_query: Query<Entity, Added<ConnectionId>>,
) {
    for player_entity in player_query.iter() {
        commands
            .entity(player_entity)
            .insert(HeldItemStack(ItemStack::default()));
    }
}

// TODO: This would have to be done for all interfaces, which is a pain. Maybe the client should
// instead send a general READY signal, and then we add all the player components and change
// detection does the rest.
//
// Send initial inventory interface content to players when they connect.
fn send_initial_inventory_interface(
    net: Res<NetworkServer>,
    players: Res<Players>,
    mut inventory_query: Query<&mut ItemStorage, With<PlayerName>>,
    mut interface_update_requests: EventReader<
        NetworkData<messages::InitialInterfaceUpdateRequest>,
    >,
) {
    for request in interface_update_requests.iter() {
        // Trigger change detection
        inventory_query
            .get_mut(players.get(&request.source))
            .unwrap()
            .into_inner();
        net.send_one(
            request.source,
            messages::InterfaceOpen {
                name: "hotbar".to_owned(),
            },
        );
    }
}

fn update_inventory_interface(
    net: Res<NetworkServer>,
    players: Res<Players>,
    recipes: Res<Recipes>,
    items: Res<Items>,
    mut take_events: EventReader<NetworkData<messages::InterfaceTakeItem>>,
    mut place_events: EventReader<NetworkData<messages::InterfacePlaceItem>>,
    mut inventory_query: ParamSet<(
        Query<(
            &mut ItemStorage,
            &mut PlayerEquipment,
            &mut PlayerInventoryCraftingTable,
            &mut HeldItemStack,
        )>,
        Query<
            (
                &mut ItemStorage,
                &mut PlayerEquipment,
                &mut PlayerInventoryCraftingTable,
                &ConnectionId,
            ),
            Changed<ItemStorage>,
        >,
    )>,
) {
    // XXX: It's important that this happens in the same system as place/take events. This way when
    // we get a take/place event from the client we only respond with an interface update if the
    // action it took was illegal. If the action is legal it will not trigger change detection, and
    // thus this loop will not send the interface update.
    for (mut changed_inventory, mut equipment, mut crafting_table, connection_id) in
        inventory_query.p1().iter_mut()
    {
        let interface = PlayerInventoryInterface {
            inventory: &mut changed_inventory,
            equipment: &mut equipment,
            crafting_table: &mut crafting_table,
            recipes: recipes.get("crafting"),
            item_configs: &items,
        };
        let interface_update = interface.build();
        net.send_one(*connection_id, interface_update.hotbar.unwrap());
        net.send_one(*connection_id, interface_update.inventory.unwrap());
    }

    let mut inventory_query_p0 = inventory_query.p0();

    for take_event in take_events.iter() {
        let player_entity = players.get(&take_event.source);
        let (mut inventory, mut equipment, mut crafting_table, mut held_item) =
            inventory_query_p0.get_mut(player_entity).unwrap();

        let mut interface = PlayerInventoryInterface {
            inventory: &mut inventory,
            equipment: &mut equipment,
            crafting_table: &mut crafting_table,
            recipes: recipes.get("crafting"),
            item_configs: &items,
        };

        let interface_update = interface.take_item(
            take_event.section,
            take_event.from_box,
            take_event.quantity,
            &mut held_item,
        );

        if let Some(inventory_update) = interface_update.inventory {
            net.send_one(take_event.source, inventory_update);
        }

        if let Some(hotbar_update) = interface_update.hotbar {
            net.send_one(take_event.source, hotbar_update);
        }
    }

    for place_event in place_events.iter() {
        let player_entity = players.get(&place_event.source);
        let (mut inventory, mut equipment, mut crafting_table, mut held_item) =
            inventory_query_p0.get_mut(player_entity).unwrap();

        if held_item.is_empty() {
            continue;
        }

        let mut interface = PlayerInventoryInterface {
            inventory: &mut inventory,
            equipment: &mut equipment,
            crafting_table: &mut crafting_table,
            recipes: recipes.get("crafting"),
            item_configs: &items,
        };

        // Quantity is only respected if the item box is empty, otherwise it replaces the held item
        // with the one in the box, returning what was there before.
        let interface_update = interface.place_item(
            place_event.section,
            place_event.to_box,
            place_event.quantity,
            &mut held_item,
        );

        if let Some(inventory_update) = interface_update.inventory {
            net.send_one(place_event.source, inventory_update);
        }

        if let Some(hotbar_update) = interface_update.hotbar {
            net.send_one(place_event.source, hotbar_update);
        }
    }
}

// TODO: The client can be bad
fn equip_item(
    net: Res<NetworkServer>,
    players: Res<Players>,
    mut equip_events: EventReader<NetworkData<messages::InterfaceEquipItem>>,
    mut equipped_item_query: Query<&mut PlayerEquippedItem>,
) {
    for equip_event in equip_events.iter() {
        if equip_event.name != "hotbar" {
            return;
        }

        if equip_event.index > 8 {
            net.disconnect(equip_event.source);
            continue;
        }

        let player_entity = players.get(&equip_event.source);
        let mut equipped_item = equipped_item_query.get_mut(player_entity).unwrap();
        equipped_item.0 = equip_event.index as usize;
    }
}
