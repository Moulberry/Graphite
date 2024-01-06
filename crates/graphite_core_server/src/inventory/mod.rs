use graphite_network::PacketBuffer;
use thiserror::Error;

use self::item_stack::ItemStack;

pub mod item_stack;

struct Slot {
    server_item: Option<ItemStack>,
    remote_item: Option<ItemStack>,
    maybe_changed: bool
}

impl Default for Slot {
    fn default() -> Self {
        Self {
            server_item: None,
            remote_item: None,
            maybe_changed: false
        }
    }
}

pub struct Inventory {
    slots: [Slot; 46],
    state_id: i32,
    maybe_changed: bool
}

impl Inventory {
    pub fn new() -> Self {
        Self {
            slots: [(); 46].map(|_| Default::default()),
            state_id: 0,
            maybe_changed: false
        }
    }

    pub fn set(&mut self, slot: InventorySlot, item_stack: Option<ItemStack>) {
        if let Ok(index) = slot.get_index() {
            self.maybe_changed = true;
            self.slots[index].maybe_changed = true;
            self.slots[index].server_item = item_stack;
        }
    }

    pub fn get(&self, slot: InventorySlot) -> Option<&ItemStack> {
        if let Ok(index) = slot.get_index() {
            self.slots[index].server_item.as_ref()
        } else {
            None
        }
    }

    pub(crate) fn mark_modified_by_client(&mut self, index: usize, remote: Option<ItemStack>) {
        self.maybe_changed = true;
        self.slots[index].maybe_changed = true;
        self.slots[index].remote_item = remote;
    }

    pub fn get_mut(&mut self, slot: InventorySlot) -> Option<&mut ItemStack> {
        if let Ok(index) = slot.get_index() {
            self.maybe_changed = true;
            self.slots[index].maybe_changed = true;
            self.slots[index].server_item.as_mut()
        } else {
            None
        }
    }

    pub fn synchronize(&mut self, packet_buffer: &mut PacketBuffer) {
        if !self.maybe_changed {
            return;
        }

        for (index, slot) in &mut self.slots.iter_mut().enumerate() {
            if slot.maybe_changed && slot.server_item != slot.remote_item {
                self.state_id += 1;

                packet_buffer.write_packet(&graphite_mc_protocol::play::clientbound::ContainerSetSlot {
                    window_id: 0,
                    state_id: self.state_id,
                    slot: index as i16,
                    item: slot.server_item.as_ref().map(|item_stack| item_stack.into()),
                }).unwrap();

                slot.maybe_changed = false;
            }
        }

        self.maybe_changed = false;
    }
}

pub enum InventorySlot {
    Hotbar(usize),
    Main(usize),
    OffHand,
    Head,
    Chest,
    Legs,
    Feet,
    CraftingInput(usize),
    CraftingResult,
}

#[derive(Debug, Error)]
#[error("slot index out of bounds: the max is {0} but the index is {1}")]
pub struct SlotOutOfBoundsError(usize, usize);

impl InventorySlot {
    pub fn from_index(slot: i16) -> std::result::Result<Self, SlotOutOfBoundsError> {
        match slot {
            36..=44 => {
                Ok(InventorySlot::Hotbar(slot as usize - 36))
            }
            9..=35 => {
                Ok(InventorySlot::Main(slot as usize - 9))
            }
            45 => Ok(InventorySlot::OffHand),
            5 => Ok(InventorySlot::Head),
            6 => Ok(InventorySlot::Chest),
            7 => Ok(InventorySlot::Legs),
            8 => Ok(InventorySlot::Feet),
            0..=3 => {
                Ok(InventorySlot::CraftingInput(slot as usize))
            }
            4 => Ok(InventorySlot::CraftingResult),
            _ => Err(SlotOutOfBoundsError(46, slot as usize))
        }
    }

    pub fn get_index(&self) -> std::result::Result<usize, SlotOutOfBoundsError> {
        match self {
            InventorySlot::Hotbar(index) => {
                if *index < 9 {
                    Ok(index + 36)
                } else {
                    Err(SlotOutOfBoundsError(9, *index))
                }
            }
            InventorySlot::Main(index) => {
                if *index < 27 {
                    Ok(index + 9)
                } else {
                    Err(SlotOutOfBoundsError(27, *index))
                }
            }
            InventorySlot::OffHand => Ok(45),
            InventorySlot::Head => Ok(5),
            InventorySlot::Chest => Ok(6),
            InventorySlot::Legs => Ok(7),
            InventorySlot::Feet => Ok(8),
            InventorySlot::CraftingInput(index) => {
                if *index < 4 {
                    Ok(*index)
                } else {
                    Err(SlotOutOfBoundsError(4, *index))
                }
            }
            InventorySlot::CraftingResult => Ok(4),
        }
    }
}