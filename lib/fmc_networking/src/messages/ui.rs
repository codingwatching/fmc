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
    /// Index of the item box in the section.
    pub item_box_id: u32,
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

// TODO: Want this removed. The server can't just send the data because the client needs time to
// process the assets after connection. Without processing the assets(which contains the
// interfaces) the client won't know where to put it. The client instead sends this when it is
// finished. It would be much cleaner if it was just implicit. The solution would be for network
// events to not be coupled to bevy events. Bevy events get cleared each update cycle, but all
// network events should be processed by the client, the clearing should happen when consumed.
/// Request an update of all enterable interfaces.
#[derive(NetworkMessage, ServerBound, Serialize, Deserialize, Debug, Clone)]
pub struct InitialInterfaceUpdateRequest;

/// Update the content of an interface.
#[derive(NetworkMessage, ClientBound, Serialize, Deserialize, Debug, Clone)]
pub struct InterfaceItemBoxUpdate {
    /// Name of the interface that should be updated
    pub name: String,
    /// Replace an old set of item boxes with new ones.
    pub replace: bool,
    /// The sections of the interface, containing the itemboxes to be updated.
    pub item_box_sections: HashMap<u32, Vec<ItemBox>>,
}

impl InterfaceItemBoxUpdate {
    pub fn new(name: &str, replace: bool) -> Self {
        return Self {
            name: name.to_owned(),
            replace,
            item_box_sections: HashMap::new(),
        };
    }

    /// Place an item in an item box
    pub fn add_itembox(
        &mut self,
        section_id: u32,
        item_box_id: u32,
        item_id: u32,
        quantity: u32,
        durability: Option<u32>,
        description: Option<&str>,
    ) {
        self.item_box_sections
            .entry(section_id)
            .or_insert(Vec::new())
            .push(ItemBox {
                item_box_id,
                item_stack: ItemStack {
                    item_id: Some(item_id),
                    quantity,
                    durability,
                    description: description.map(|x| x.to_owned()),
                },
            })
    }

    /// Empty the contents of an itembox
    pub fn add_empty_itembox(&mut self, section_id: u32, item_box_id: u32) {
        //self.item_boxes.push((section, box_id, None));
        self.item_box_sections
            .entry(section_id)
            .or_insert(Vec::new())
            .push(ItemBox {
                item_box_id,
                item_stack: ItemStack {
                    item_id: None,
                    quantity: 0,
                    durability: None,
                    description: None,
                },
            })
    }

    pub fn combine(&mut self, mut other: InterfaceItemBoxUpdate) {
        for (section, boxes) in other.item_box_sections.iter_mut() {
            self.item_box_sections
                .entry(*section)
                .or_insert(Vec::new())
                .append(boxes);
        }
    }
}

/// Take an item from an item box
#[derive(NetworkMessage, ServerBound, Serialize, Deserialize, Debug, Clone)]
pub struct InterfaceTakeItem {
    /// Name of the interface that is interacted with
    pub name: String,
    /// Section of the item box.
    pub section: u32,
    /// Item box that the item should be removed from
    pub from_box: u32,
    /// Quantity of the item that should be moved.
    pub quantity: u32,
}

/// Place an item in an item box
#[derive(NetworkMessage, ServerBound, Serialize, Deserialize, Debug, Clone)]
pub struct InterfacePlaceItem {
    /// Name of the interface that is interacted with
    pub name: String,
    /// Section of the item box.
    pub section: u32,
    /// Item box that the item should be removed from
    pub to_box: u32,
    /// Quantity of the item that should be moved.
    pub quantity: u32,
}

/// Equip the item in the specified interface
#[derive(NetworkMessage, ServerBound, Serialize, Deserialize, Debug, Clone)]
pub struct InterfaceEquipItem {
    /// Name of the interface
    pub name: String,
    /// Section of the item box.
    pub section: u32,
    /// Item box index
    pub index: u32,
}
