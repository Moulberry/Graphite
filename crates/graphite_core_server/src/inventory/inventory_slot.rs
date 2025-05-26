use graphite_mc_constants::builtin::ContainerType;
use once_cell::sync::Lazy;
use thiserror::Error;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum InventorySlot {
    Container(u8),
    Hotbar(u8),
    Main(u8),
    OffHand,
    Head,
    Chest,
    Legs,
    Feet,
    CraftingInput(u8),
    CraftingResult,
    Carried
}

#[derive(Debug, Error)]
#[error("slot index out of bounds")]
pub struct SlotOutOfBoundsError;

static SLOTS_FOR_CONTAINER: Lazy<[Vec<InventorySlot>; ContainerType::COUNT]> = Lazy::new(|| {
    std::array::from_fn(|index| {
        let container_type = ContainerType::try_from(index as u8).unwrap();

        let mut slots = Vec::new();
        for i in 0..128 {
            if let Ok(slot) = InventorySlot::from_index(i, Some(container_type)) {
                slots.push(slot);
            } else {
                break;
            }
        }
        slots.shrink_to_fit();
        slots
    })
});
static SLOTS_FOR_INVENTORY: Lazy<Vec<InventorySlot>> = Lazy::new(|| {
    let mut slots = Vec::new();
    for i in 0..128 {
        if let Ok(slot) = InventorySlot::from_index(i, None) {
            slots.push(slot);
        } else {
            break;
        }
    }
    slots.shrink_to_fit();
    slots
});

impl InventorySlot {
    pub fn all_main() -> impl DoubleEndedIterator<Item = InventorySlot> {
        (0..27).map(|i| InventorySlot::Main(i))
    }

    pub fn all_hotbar() -> impl DoubleEndedIterator<Item = InventorySlot> {
        (0..9).map(|i| InventorySlot::Hotbar(i))
    }

    pub fn main_and_hotbar() -> impl DoubleEndedIterator<Item = InventorySlot> {
        Self::all_main().chain(Self::all_hotbar())
    }

    pub fn all_for_container(container_type: Option<ContainerType>) -> &'static [Self] {
        if let Some(container_type) = container_type {
            &SLOTS_FOR_CONTAINER[container_type as usize]
        } else {
            &SLOTS_FOR_INVENTORY
        }
    }

    pub fn from_index(slot: i16, container_type: Option<ContainerType>) -> std::result::Result<Self, SlotOutOfBoundsError> {
        if slot < 0 {
            return Err(SlotOutOfBoundsError);
        }
        let slot = slot as usize;

        if let Some(container_type) = container_type {
            let size = container_type.slot_count();

            // Assume that a slot count of >=36 means that the player inventory is included
            if size >= 36 {
                if slot < size - 36 {
                    Ok(InventorySlot::Container(slot as u8))
                } else if slot < size - 9 {
                    Ok(InventorySlot::Main((slot - size + 36) as u8))
                } else if slot < size {
                    Ok(InventorySlot::Hotbar((slot - size + 9) as u8))
                } else {
                    Err(SlotOutOfBoundsError)
                }
            } else {
                if slot < size {
                    Ok(InventorySlot::Container(slot as u8))
                } else {
                    Err(SlotOutOfBoundsError)
                }
            }
        } else {
            match slot {
                36..=44 => {
                    Ok(InventorySlot::Hotbar((slot - 36) as u8))
                }
                9..=35 => {
                    Ok(InventorySlot::Main((slot - 9) as u8))
                }
                45 => Ok(InventorySlot::OffHand),
                5 => Ok(InventorySlot::Head),
                6 => Ok(InventorySlot::Chest),
                7 => Ok(InventorySlot::Legs),
                8 => Ok(InventorySlot::Feet),
                1..=4 => {
                    Ok(InventorySlot::CraftingInput((slot - 1) as u8))
                }
                0 => Ok(InventorySlot::CraftingResult),
                _ => Err(SlotOutOfBoundsError)
            }
        }
    }

    pub const fn get_index(self, container_type: Option<ContainerType>) -> std::result::Result<usize, SlotOutOfBoundsError> {
        if let Some(container_type) = container_type {
            let size = container_type.slot_count();

            // Assume that a slot count of >=36 means that the player inventory is included
            if size >= 36 {
                match self {
                    InventorySlot::Container(slot) if (slot as usize) < size - 36 => Ok(slot as usize),
                    InventorySlot::Main(slot) if slot < 27 => Ok(slot as usize + size - 36),
                    InventorySlot::Hotbar(slot) if slot < 9 => Ok(slot as usize + size - 9),
                    InventorySlot::Carried => Ok(size),
                    _ => Err(SlotOutOfBoundsError)
                }
            } else {
                match self {
                    InventorySlot::Container(slot) if (slot as usize) < size => Ok(slot as usize),
                    InventorySlot::Carried => Ok(size),
                    _ => Err(SlotOutOfBoundsError)
                }
            }
        } else {
            match self {
                InventorySlot::Hotbar(index) => {
                    if index < 9 {
                        Ok((index + 36) as usize)
                    } else {
                        Err(SlotOutOfBoundsError)
                    }
                }
                InventorySlot::Main(index) => {
                    if index < 27 {
                        Ok((index + 9) as usize)
                    } else {
                        Err(SlotOutOfBoundsError)
                    }
                }
                InventorySlot::Container(_) => Err(SlotOutOfBoundsError),
                InventorySlot::OffHand => Ok(45),
                InventorySlot::Head => Ok(5),
                InventorySlot::Chest => Ok(6),
                InventorySlot::Legs => Ok(7),
                InventorySlot::Feet => Ok(8),
                InventorySlot::CraftingInput(index) => {
                    if index < 4 {
                        Ok((index + 1) as usize)
                    } else {
                        Err(SlotOutOfBoundsError)
                    }
                }
                InventorySlot::CraftingResult => Ok(0),
                InventorySlot::Carried => Ok(46)
            }
        }
    }
}