use std::result;


use graphite_mc_constants::item::NoSuchItemError;
use graphite_net::{network_buffer::WriteBuffer, packet_helper};
use graphite_mc_protocol::play::server::ContainerSetSlot;
use graphite_mc_protocol::types::ProtocolItemStack;
use thiserror::Error;

use super::itemstack::ItemStack;

pub trait InventoryHandler: Default {
    fn creative_mode_set(
        &mut self,
        index: usize,
        itemstack: Option<ProtocolItemStack>,
    ) -> anyhow::Result<()>;
    fn get(&self, slot: InventorySlot) -> result::Result<&ItemSlot, SlotOutOfBoundsError>;
    fn set(
        &mut self,
        slot: InventorySlot,
        itemstack: ItemStack,
    ) -> result::Result<(), SlotOutOfBoundsError>;
    fn clear(&mut self, slot: InventorySlot) -> result::Result<(), SlotOutOfBoundsError>;

    fn has_changed(&self, slot: InventorySlot) -> result::Result<bool, SlotOutOfBoundsError>;
    fn is_any_changed(&self) -> bool;
    fn write_changes(&mut self, write_buffer: &mut WriteBuffer) -> result::Result<(), ItemTooBig>;
}

#[derive(Default, Clone, Debug)]
pub enum ItemSlot {
    #[default]
    Empty,
    Filled(ItemStack),
}

impl<'a> TryFrom<Option<ProtocolItemStack<'a>>> for ItemSlot {
    type Error = NoSuchItemError;
    fn try_from(value: Option<ProtocolItemStack>) -> Result<Self, Self::Error> {
        match value {
            Some(itemstack) => Ok(ItemSlot::Filled(itemstack.try_into()?)),
            None => Ok(ItemSlot::Empty),
        }
    }
}

impl<'a> From<&'a ItemSlot> for Option<ProtocolItemStack<'a>> {
    fn from(value: &'a ItemSlot) -> Self {
        match value {
            ItemSlot::Filled(itemstack) => Some(itemstack.into()),
            ItemSlot::Empty => None,
        }
    }
}

pub struct VanillaPlayerInventory {
    change_state: ChangeState,
    slots: [ItemSlot; 46],
}

impl Default for VanillaPlayerInventory {
    fn default() -> Self {
        Self {
            change_state: ChangeState::NoChange,
            slots: [(); 46].map(|_| Default::default()),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum InventorySlot {
    All(usize),
    Hotbar(usize),
    Main(usize),
    MainAndHotbar(usize),
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

#[derive(Debug, Error)]
#[error("item is too big to send (exceeds 2mb)")]
pub struct ItemTooBig;

impl InventorySlot {
    pub fn get_index(&self) -> result::Result<usize, SlotOutOfBoundsError> {
        match self {
            InventorySlot::All(index) => {
                if *index < 46 {
                    Ok(*index)
                } else {
                    Err(SlotOutOfBoundsError(46, *index))
                }
            }
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
            InventorySlot::MainAndHotbar(index) => {
                if *index < 36 {
                    Ok(index + 9)
                } else {
                    Err(SlotOutOfBoundsError(36, *index))
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

// Whether the change was triggered by the client or server
// Changes triggered by the client are not forwarded back to the client
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum ChangeType {
    None,
    Client,
    Server,
}

enum ChangeState {
    NoChange,
    SingleSlot {
        index: usize,
        change_type: ChangeType,
    },
    MultiSlot {
        changed: [ChangeType; 46],
    },
}

impl InventoryHandler for VanillaPlayerInventory {
    fn creative_mode_set(
        &mut self,
        index: usize,
        itemstack: Option<ProtocolItemStack>,
    ) -> anyhow::Result<()> {
        if index > 45 {
            return Err(SlotOutOfBoundsError(45, index).into());
        }

        let slot = match itemstack {
            Some(itemstack) => ItemSlot::Filled(itemstack.try_into()?),
            None => ItemSlot::Empty,
        };
        self.slots[index] = slot;
        self.mark_changed(index, ChangeType::Client);

        Ok(())
    }

    fn get(&self, slot: InventorySlot) -> result::Result<&ItemSlot, SlotOutOfBoundsError> {
        let index = slot.get_index()?;
        Ok(&self.slots[index])
    }

    fn set(
        &mut self,
        slot: InventorySlot,
        itemstack: ItemStack,
    ) -> result::Result<(), SlotOutOfBoundsError> {
        let index = slot.get_index()?;
        self.slots[index] = ItemSlot::Filled(itemstack);
        self.mark_changed(index, ChangeType::Server);
        Ok(())
    }

    fn clear(&mut self, section: InventorySlot) -> result::Result<(), SlotOutOfBoundsError> {
        let slot = section.get_index()?;
        self.slots[slot] = ItemSlot::Empty;
        self.mark_changed(slot, ChangeType::Server);
        Ok(())
    }

    fn has_changed(&self, slot: InventorySlot) -> result::Result<bool, SlotOutOfBoundsError> {
        let check_index = slot.get_index()?;
        match self.change_state {
            ChangeState::NoChange => Ok(false),
            ChangeState::SingleSlot { index, change_type } => {
                Ok(check_index == index && change_type != ChangeType::None)
            }
            ChangeState::MultiSlot { changed } => Ok(changed[check_index] != ChangeType::None),
        }
    }

    fn is_any_changed(&self) -> bool {
        match self.change_state {
            ChangeState::NoChange => false,
            ChangeState::SingleSlot {
                index: _,
                change_type: _,
            }
            | ChangeState::MultiSlot { changed: _ } => true,
        }
    }

    fn write_changes(&mut self, write_buffer: &mut WriteBuffer) -> result::Result<(), ItemTooBig> {
        match self.change_state {
            ChangeState::NoChange => return Ok(()),
            ChangeState::SingleSlot { index, change_type } => {
                if change_type == ChangeType::Server {
                    self.send_container_slot(index, write_buffer)?;
                }
            }
            ChangeState::MultiSlot { changed } => {
                for (index, change_type) in changed.iter().enumerate() {
                    if *change_type == ChangeType::Server {
                        self.send_container_slot(index, write_buffer)?;
                    }
                }
            }
        }

        self.change_state = ChangeState::NoChange;

        Ok(())
    }
}

impl VanillaPlayerInventory {
    fn send_container_slot(
        &self,
        index: usize,
        write_buffer: &mut WriteBuffer,
    ) -> result::Result<(), ItemTooBig> {
        let item = (&self.slots[index]).into();

        let packet = ContainerSetSlot {
            window_id: 0,
            state_id: 0,
            slot: index as _,
            item,
        };

        if packet_helper::write_packet(write_buffer, &packet).is_err() {
            Err(ItemTooBig)
        } else {
            Ok(())
        }
    }

    fn mark_changed(&mut self, index: usize, change_type: ChangeType) {
        match &mut self.change_state {
            ChangeState::NoChange => {
                self.change_state = ChangeState::SingleSlot { index, change_type }
            }
            ChangeState::SingleSlot {
                index: other_index,
                change_type: other_change_type,
            } => {
                if *other_index != index {
                    let mut changed = [ChangeType::None; 46];
                    changed[*other_index] = *other_change_type;
                    changed[index] = change_type;

                    self.change_state = ChangeState::MultiSlot { changed }
                }
            }
            ChangeState::MultiSlot { changed } => {
                if changed[index] < change_type {
                    changed[index] = change_type;
                }
            }
        }
    }
}
