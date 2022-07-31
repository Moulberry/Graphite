use std::result;

use net::{network_buffer::WriteBuffer, packet_helper};
use protocol::{play::server::ContainerSetSlot};
use protocol::types::ProtocolItemStack;
use thiserror::Error;

use super::itemstack::ItemStack;

pub trait InventoryHandler: Default {
    type InventorySection;

    fn creative_mode_set(&mut self, index: usize, itemstack: Option<ProtocolItemStack>) -> anyhow::Result<()>;
    fn get(&self, section: Self::InventorySection) -> result::Result<&ItemSlot, SlotOutOfBoundsError>;
    fn set(&mut self, section: Self::InventorySection, itemstack: ItemStack) -> result::Result<(), SlotOutOfBoundsError>;
    fn clear(&mut self, section: Self::InventorySection) -> result::Result<(), SlotOutOfBoundsError>;
    fn write_changes(&mut self, write_buffer: &mut WriteBuffer) -> anyhow::Result<()>;
}

#[derive(Default, Clone, Debug)]
pub enum ItemSlot {
    #[default]
    Empty,
    Filled(ItemStack)
}

pub struct VanillaPlayerInventory {
    change_state: ChangeState,
    slots: [ItemSlot; 46]
}

impl Default for VanillaPlayerInventory {
    fn default() -> Self {
        Self {
            change_state: ChangeState::NoChange,
            slots: [(); 46].map(|_| Default::default())
        }
    }
}

pub enum PlayerInventorySection {
    All(usize),
    Hotbar(usize),
    Main(usize),
    MainAndHotbar(usize),
    OffHand,
    Helmet,
    Chestplate,
    Leggings,
    Boots,
    CraftingInput(usize),
    CraftingResult
}

#[derive(Debug, Error)]
#[error("slot index out of bounds: the max is {0} but the index is {1}")]
pub struct SlotOutOfBoundsError(usize, usize);

impl PlayerInventorySection {
    pub fn get_slot(&self) -> result::Result<usize, SlotOutOfBoundsError> {
        match self {
            PlayerInventorySection::All(index) => {
                if *index < 46 {
                    Ok(*index)
                } else {
                    Err(SlotOutOfBoundsError(46, *index))
                }
            },
            PlayerInventorySection::Hotbar(index) => {
                if *index < 9 {
                    Ok(index + 36)
                } else {
                    Err(SlotOutOfBoundsError(9, *index))
                }
            },
            PlayerInventorySection::Main(index) => {
                if *index < 27 {
                    Ok(index + 9)
                } else {
                    Err(SlotOutOfBoundsError(27, *index))
                }
            },
            PlayerInventorySection::MainAndHotbar(index) => {
                if *index < 36 {
                    Ok(index + 9)
                } else {
                    Err(SlotOutOfBoundsError(36, *index))
                }
            },
            PlayerInventorySection::OffHand => Ok(45),
            PlayerInventorySection::Helmet => Ok(5),
            PlayerInventorySection::Chestplate => Ok(6),
            PlayerInventorySection::Leggings => Ok(7),
            PlayerInventorySection::Boots => Ok(8),
            PlayerInventorySection::CraftingInput(index) => {
                if *index < 4 {
                    Ok(*index)
                } else {
                    Err(SlotOutOfBoundsError(4, *index))
                }
            },
            PlayerInventorySection::CraftingResult => Ok(4),
        }
    }
}

enum ChangeState {
    NoChange,
    SingleSlot {
        slot: usize
    },
    MultiSlot {
        count: usize,
        changed: [bool; 46] // todo: maybe don't waste 40 bytes here
    }
}

impl InventoryHandler for VanillaPlayerInventory {
    type InventorySection = PlayerInventorySection;

    fn creative_mode_set(&mut self, index: usize, itemstack: Option<ProtocolItemStack>) -> anyhow::Result<()> {
        if index > 45 {
            return Err(SlotOutOfBoundsError(45, index).into())
        }

        let slot = match itemstack {
            Some(itemstack) => ItemSlot::Filled(itemstack.try_into()?),
            None => ItemSlot::Empty,
        };
        self.slots[index] = slot;
        self.mark_changed(index);

        Ok(())
    }

    fn get(&self, section: Self::InventorySection) -> result::Result<&ItemSlot, SlotOutOfBoundsError> {
        let slot = section.get_slot()?;
        Ok(&self.slots[slot])
    }

    fn set(&mut self, section: Self::InventorySection, itemstack: ItemStack) -> result::Result<(), SlotOutOfBoundsError> {
        let slot = section.get_slot()?;
        self.slots[slot] = ItemSlot::Filled(itemstack);
        self.mark_changed(slot);
        Ok(())
    }

    fn clear(&mut self, section: Self::InventorySection) -> result::Result<(), SlotOutOfBoundsError> {
        let slot = section.get_slot()?;
        self.slots[slot] = ItemSlot::Empty;
        self.mark_changed(slot);
        Ok(())
    }

    fn write_changes(&mut self, write_buffer: &mut WriteBuffer) -> anyhow::Result<()> {
        match self.change_state {
            ChangeState::NoChange => return Ok(()),
            ChangeState::SingleSlot { slot } => {
                let item = match &self.slots[slot] {
                    ItemSlot::Empty => {
                        None
                    },
                    ItemSlot::Filled(itemstack) => {
                        Some(
                            ProtocolItemStack {
                                item: itemstack.item as _,
                                count: itemstack.count,
                                temp_nbt: 0,
                            }
                        )
                    },
                };

                let packet = ContainerSetSlot {
                    window_id: 0,
                    state_id: 0,
                    slot: slot as _,
                    item
                };
                packet_helper::write_packet(write_buffer, &packet)?;        
            },
            ChangeState::MultiSlot { count: _, changed: _ } => {
                todo!();
            },
        }

        self.change_state = ChangeState::NoChange;

        Ok(())
    }
}

impl VanillaPlayerInventory {
    fn mark_changed(&mut self, index: usize) {
        match self.change_state {
            ChangeState::NoChange => {
                self.change_state = ChangeState::SingleSlot { slot: index }
            },
            ChangeState::SingleSlot { slot: other_index } => {
                if other_index != index {
                    let mut changed = [false; 46];
                    changed[other_index] = true;
                    changed[index] = true;

                    self.change_state = ChangeState::MultiSlot {
                        count: 2,
                        changed
                    }
                }
            },
            ChangeState::MultiSlot { count, mut changed } => {
                if !changed[index] {
                    changed[index] = true;
                    
                    self.change_state = ChangeState::MultiSlot {
                        count: count + 1,
                        changed
                    }
                }
            }
        }
    }
}