use fmc_networking_derive::{ClientBound, NetworkMessage, ServerBound};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Close an interface that is currently closed.
#[derive(NetworkMessage, ClientBound, Serialize, Deserialize, Debug, Clone)]
pub struct InterfaceOpen {
    /// Name of the interface that should be opened.
    pub name: String,
}

/// Close an interface that is currently open.
#[derive(NetworkMessage, ClientBound, Serialize, Deserialize, Debug, Clone)]
pub struct InterfaceClose {
    /// Name of the interface that should be closed.
    pub name: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ItemBox {
    /// Index of the item box in the interface.
    pub index: u32,
    /// Item stack that should be used, if no item id is given, the box will be empty.
    pub item_stack: ItemStack,
}

#[derive(Default, Serialize, Deserialize, Clone, Debug)]
pub struct ItemStack {
    /// Item id
    pub item_id: Option<u32>,
    /// Number of items
    pub quantity: u32,
    /// Durability of item
    pub durability: Option<u32>,
    /// Description of item
    pub description: Option<String>,
}

/// Update the content of an interface.
#[derive(NetworkMessage, ClientBound, Serialize, Deserialize, Debug, Clone)]
pub struct InterfaceItemBoxUpdate {
    /// Remove the previous item boxes before adding these. If this is true, the updates are
    /// assumed to be ordered. The index will be ignored.
    pub replace: bool,
    /// The sections of the interface, containing the itemboxes to be updated.
    pub updates: HashMap<String, Vec<ItemBox>>,
}

impl InterfaceItemBoxUpdate {
    pub fn new(replace: bool) -> Self {
        return Self {
            replace,
            updates: HashMap::new(),
        };
    }

    /// Place an item in an item box
    pub fn add_itembox(
        &mut self,
        name: &str,
        item_box_id: u32,
        item_id: u32,
        quantity: u32,
        durability: Option<u32>,
        description: Option<&str>,
    ) {
        if !self.updates.contains_key(name) {
            self.updates.insert(name.to_owned(), Vec::new());
        }

        self.updates.get_mut(name).unwrap()
            .push(ItemBox {
                index: item_box_id,
                item_stack: ItemStack {
                    item_id: Some(item_id),
                    quantity,
                    durability,
                    description: description.map(|x| x.to_owned()),
                },
            })
    }

    /// Empty the contents of an itembox
    pub fn add_empty_itembox(&mut self, name: &str, item_box_id: u32) {
        if !self.updates.contains_key(name) {
            self.updates.insert(name.to_owned(), Vec::new());
        }
        self.updates.get_mut(name).unwrap()
            .push(ItemBox {
                index: item_box_id,
                item_stack: ItemStack {
                    item_id: None,
                    quantity: 0,
                    durability: None,
                    description: None,
                },
            })
    }

    pub fn combine(&mut self, other: InterfaceItemBoxUpdate) {
        for (interface_name, mut updates) in other.updates.into_iter() {
            if self.updates.contains_key(&interface_name) {
                self.updates.get_mut(&interface_name).unwrap().append(&mut updates);
            } else {
                self.updates.insert(interface_name, updates);
            }
        }
    }
}

/// Take an item from an item box
#[derive(NetworkMessage, ServerBound, Serialize, Deserialize, Debug, Clone)]
pub struct InterfaceTakeItem {
    /// Interface identifier, formatted like "root/child/grandchild/..etc", e.g.
    /// "inventory/crafting_table"
    pub interface_path: String,
    /// Item box that the item should be removed from
    pub from_box: u32,
    /// Quantity of the item that should be moved.
    pub quantity: u32,
}

/// Place an item in an item box
#[derive(NetworkMessage, ServerBound, Serialize, Deserialize, Debug, Clone)]
pub struct InterfacePlaceItem {
    /// Interface identifier, formatted like "root/child/grandchild/..etc", e.g.
    /// "inventory/crafting_table"
    pub interface_path: String,
    /// Item box that the item should be removed from
    pub to_box: u32,
    /// Quantity of the item that should be moved.
    pub quantity: u32,
}

/// Equip the item in the specified interface
#[derive(NetworkMessage, ServerBound, Serialize, Deserialize, Debug, Clone)]
pub struct InterfaceEquipItem {
    /// Interface identifier, formatted like "root/child/grandchild/..etc", e.g.
    /// "inventory/crafting_table"
    pub interface_path: String,
    /// Item box index
    pub index: u32,
}
