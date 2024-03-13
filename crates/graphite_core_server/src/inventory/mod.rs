use std::array;

use graphite_network::PacketBuffer;
use thiserror::Error;

use self::item_stack::ItemStack;

pub mod item_stack;

struct Slot<H: ItemHolder> {
    server_item: H,
    remote_item: Option<ItemStack>,
    maybe_changed: bool
}

impl <H: ItemHolder> Slot<H> {
    fn new(slot: InventorySlot) -> Self {
        Self {
            server_item: H::create_empty(slot),
            remote_item: Some(ItemStack::EMPTY),
            maybe_changed: true
        }
    }
}

pub trait ItemHolder {
    fn get_item_stack(&self) -> &ItemStack;
    fn create_empty(slot: InventorySlot) -> Self;
    fn create_out_of_bounds() -> Self;
}

impl ItemHolder for ItemStack {
    fn get_item_stack(&self) -> &ItemStack {
        self
    }

    fn create_empty(slot: InventorySlot) -> Self {
        ItemStack::EMPTY
    }

    fn create_out_of_bounds() -> Self {
        ItemStack::EMPTY
    }
}

pub struct Inventory<H: ItemHolder> {
    slots: [Slot<H>; 46],
    state_id: i32,
    maybe_changed: bool,
    out_of_bounds: H
}

impl <H: ItemHolder> Inventory<H> {
    pub fn new() -> Self {
        Self {
            slots: array::from_fn(|index| Slot::new(InventorySlot::from_index(index as i16).unwrap())),
            state_id: 0,
            maybe_changed: true,
            out_of_bounds: H::create_out_of_bounds()
        }
    }

    pub fn set(&mut self, slot: InventorySlot, holder: H) {
        if let Ok(index) = slot.get_index() {
            self.maybe_changed = true;
            self.slots[index].maybe_changed = true;
            self.slots[index].server_item = holder;
        }
    }

    pub fn get(&self, slot: InventorySlot) -> &H {
        if let Ok(index) = slot.get_index() {
            &self.slots[index].server_item
        } else {
            &self.out_of_bounds
        }
    }

    pub fn get_mut(&mut self, slot: InventorySlot) -> &mut H {
        if let Ok(index) = slot.get_index() {
            self.maybe_changed = true;
            self.slots[index].maybe_changed = true;
            &mut self.slots[index].server_item
        } else {
            &mut self.out_of_bounds
        }
    }

    pub fn take(&mut self, slot: InventorySlot) -> H {
        if let Ok(index) = slot.get_index() {
            self.maybe_changed = true;
            self.slots[index].maybe_changed = true;

            let mut swap = H::create_empty(slot);
            std::mem::swap(&mut swap, &mut self.slots[index].server_item);
            swap
        } else {
            H::create_out_of_bounds()
        }
    }

    pub fn get_item_stack(&self, slot: InventorySlot) -> &ItemStack {
        self.get(slot).get_item_stack()
    }

    pub fn mark_modified_by_client(&mut self, index: usize, remote: Option<ItemStack>) {
        self.maybe_changed = true;
        self.slots[index].maybe_changed = true;
        self.slots[index].remote_item = remote;
    }

    // pub fn get_mut(&mut self, slot: InventorySlot) -> Option<&mut ItemStack> {
    //     if let Ok(index) = slot.get_index() {
    //         self.maybe_changed = true;
    //         self.slots[index].maybe_changed = true;
    //         self.slots[index].server_item.as_mut()
    //     } else {
    //         None
    //     }
    // }

    pub fn synchronize(&mut self, packet_buffer: &mut PacketBuffer) {
        if !self.maybe_changed {
            return;
        }

        for (index, slot) in &mut self.slots.iter_mut().enumerate() {
            if slot.maybe_changed {
                let mut update = false;
                if slot.remote_item.is_none() {
                    update = true;
                } else if let Some(known) = &slot.remote_item {
                    update = known != slot.server_item.get_item_stack();
                }

                if update {
                    self.state_id += 1;

                    let item_stack = slot.server_item.get_item_stack().clone();

                    packet_buffer.write_packet(&graphite_mc_protocol::play::clientbound::ContainerSetSlot {
                        window_id: 0,
                        state_id: self.state_id,
                        slot: index as i16,
                        item: (&item_stack).into(),
                    }).unwrap();
    
                    slot.remote_item = Some(item_stack);
                    slot.maybe_changed = false;
                }
            }
        }

        self.maybe_changed = false;
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
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