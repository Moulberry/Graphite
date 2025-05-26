use std::i32;

use graphite_mc_constants::types::EquipmentSlot;
use graphite_mc_protocol::types::ItemStack;

use crate::player::{DropCause, PlayerExtension};

use super::{inventory_slot::InventorySlot, item_holder::{ItemHolder, ItemHolderRefMut}, menu::ContainerAction, player_container_view::PlayerFunc};

pub trait Container {
    type Item: ItemHolder;

    fn take(&mut self, slot: InventorySlot) -> Option<Self::Item>;
    fn insert(&mut self, slot: InventorySlot, item: Self::Item);
    fn get(&self, slot: InventorySlot) -> Option<&Self::Item>;
    fn get_item_stack(&self, slot: InventorySlot) -> Option<ItemStack> {
        self.get(slot).map(Self::Item::get_item_stack)
    }
    fn is_item_compatible_with(&self, item: &Self::Item, slot: InventorySlot) -> bool;
}

pub fn handle_container_action_for_inventory<P: PlayerExtension>(inventory: &mut P::InventoryContainer, action: ContainerAction) -> PlayerFunc<P> {
    match action {
        ContainerAction::Click { slot: clicked_slot, shift_down, right_click } => {
            if shift_down {
                let Some(mut clicked) = inventory.take(clicked_slot) else {
                    return None;
                };

                match clicked_slot {
                    InventorySlot::Carried => {},
                    InventorySlot::Head | InventorySlot::Chest | InventorySlot::Legs | InventorySlot::Feet | InventorySlot::CraftingInput(_) => {
                        shift_move_items(inventory, &mut clicked, InventorySlot::main_and_hotbar())
                    },
                    InventorySlot::CraftingResult => shift_move_items(inventory, &mut clicked, InventorySlot::main_and_hotbar().rev()),
                    _ => {
                        if let Some(equipment_slot) = clicked.get_item_stack().get_equipment_slot() {
                            let slot = match equipment_slot {
                                EquipmentSlot::Offhand => Some(InventorySlot::OffHand),
                                EquipmentSlot::Feet => Some(InventorySlot::Feet),
                                EquipmentSlot::Legs => Some(InventorySlot::Legs),
                                EquipmentSlot::Chest => Some(InventorySlot::Chest),
                                EquipmentSlot::Head => Some(InventorySlot::Head),
                                _ => None
                            };
                            if let Some(target_slot) = slot {
                                if inventory.is_item_compatible_with(&clicked, target_slot) {
                                    let target = inventory.take(target_slot);
                                    if let Some(mut target) = target {
                                        if clicked.transfer(&mut target, i32::MAX) != 0 {
                                            inventory.insert(target_slot, target);
                                            inventory.insert(clicked_slot, clicked);
                                            return None;
                                        }
                                    };
                                }
                            }
                        }

                        match clicked_slot {
                            InventorySlot::Hotbar(_) => shift_move_items(inventory, &mut clicked, InventorySlot::all_main()),
                            InventorySlot::Main(_) => shift_move_items(inventory, &mut clicked, InventorySlot::all_hotbar()),
                            _ => {
                                shift_move_items(inventory, &mut clicked, InventorySlot::main_and_hotbar())
                            }
                        }

                    }
                }

                inventory.insert(clicked_slot, clicked);
            } else {
                let Some(mut clicked) = inventory.take(clicked_slot) else {
                    return None;
                };

                if !inventory.is_item_compatible_with(&clicked, InventorySlot::Carried) {
                    inventory.insert(clicked_slot, clicked);
                    return None;
                }

                let Some(mut carried) = inventory.take(InventorySlot::Carried) else {
                    inventory.insert(clicked_slot, clicked);
                    return None;
                };

                if !inventory.is_item_compatible_with(&carried, clicked_slot) {
                    inventory.insert(clicked_slot, clicked);
                    inventory.insert(InventorySlot::Carried, carried);
                    return None;
                }

                let clicked_count = clicked.visual_count();
                let carried_count = carried.visual_count();

                if clicked_count > 0 || carried_count > 0 {
                    if right_click {
                        let transferred = if carried_count <= 0 && clicked_count > 0 {
                            // Try pickup half from click to carried
                            let transfer = (clicked_count + 1) / 2;
                            clicked.transfer(&mut carried, transfer)
                        } else if carried_count > 0 {
                            // Try deposit 1 item from carried to click
                            carried.transfer(&mut clicked, 1)
                        } else {
                            0
                        };
    
                        if transferred == 0 {
                            std::mem::swap(&mut clicked, &mut carried);
                        }
                    } else {
                        if clicked_count <= 0 || carried_count <= 0 {
                            std::mem::swap(&mut clicked, &mut carried);
                        } else if carried.transfer(&mut clicked, i32::MAX) == 0 {
                            // Fallback to swapping
                            std::mem::swap(&mut clicked, &mut carried);
                        }
                    }
                }

                inventory.insert(clicked_slot, clicked);
                inventory.insert(InventorySlot::Carried, carried);
            }
        },
        ContainerAction::Swap(from_slot, to_slot) => {
            let Some(mut from) = inventory.take(from_slot) else {
                return None;
            };

            if !inventory.is_item_compatible_with(&from, to_slot) {
                inventory.insert(from_slot, from);
                return None;
            }

            let Some(mut to) = inventory.take(to_slot) else {
                inventory.insert(from_slot, from);
                return None;
            };

            if inventory.is_item_compatible_with(&to, from_slot) {
                std::mem::swap(&mut from, &mut to);
            }

            inventory.insert(from_slot, from);
            inventory.insert(to_slot, to);
        },
        ContainerAction::Drag { slots, right_click } => {
            if slots.is_empty() {
                return None;
            }

            let Some(mut carried) = inventory.take(InventorySlot::Carried) else {
                return None;
            };

            let mut transfer_count = carried.visual_count() / slots.len() as i32;
            if right_click {
                transfer_count = transfer_count.min(1);
            }

            if transfer_count > 0 {
                for &slot in slots.iter() {
                    if !inventory.is_item_compatible_with(&mut carried, slot) {
                        continue;
                    }
    
                    let Some(mut item) = inventory.take(slot) else {
                        continue;
                    };
    
                    carried.transfer(&mut item, transfer_count);
    
                    inventory.insert(slot, item);
                }
            }

            inventory.insert(InventorySlot::Carried, carried);
        },
        ContainerAction::PickupAll => {
            let Some(mut carried) = inventory.take(InventorySlot::Carried) else {
                return None;
            };

            if carried.visual_count() <= 0 {
                inventory.insert(InventorySlot::Carried, carried);
                return None;
            }

            for &slot in InventorySlot::all_for_container(None) {
                if !inventory.is_item_compatible_with(&mut carried, slot) {
                    continue;
                }

                let Some(mut item) = inventory.take(slot) else {
                    continue;
                };

                item.transfer(&mut carried, i32::MAX);

                inventory.insert(slot, item);
            }

            inventory.insert(InventorySlot::Carried, carried);
        },
        ContainerAction::Throw { slot, all, outside } => {
            let Some(mut item) = inventory.take(slot) else {
                return None;
            };

            let mut thrown = item.clone_empty();
            if all {
                item.transfer(&mut thrown, i32::MAX);
            } else {
                item.transfer(&mut thrown, 1);
            }

            inventory.insert(slot, item);

            let cause: DropCause = if outside {
                DropCause::InventoryOutside
            } else {
                DropCause::Inventory
            };
            return Some(Box::new(move |player| {
                P::drop_item(player, thrown, cause);
            }));
        }
        ContainerAction::Clone(_) => {},
        ContainerAction::SelectTrade(_) => {},
        ContainerAction::SelectBundleItem { slot: _, selected: _ } => {}
    }
    return None;
}

fn shift_move_items<C: Container>(inventory: &mut C, clicked: &mut C::Item, slots: impl DoubleEndedIterator<Item = InventorySlot>) {
    let mut first_empty_slot = None;

    for target_slot in slots {
        if !inventory.is_item_compatible_with(&clicked, target_slot) {
            continue;
        }

        let target = inventory.take(target_slot);
        let Some(mut target) = target else {
            continue;
        };

        let visual_count = target.visual_count();
        if visual_count > 0 {
            clicked.transfer(&mut target, i32::MAX);
            if clicked.visual_count() <= 0 {
                inventory.insert(target_slot, target);
                return;
            }
        } else if first_empty_slot.is_none() {
            first_empty_slot = Some(target_slot);
        }

        inventory.insert(target_slot, target);
    }

    if let Some(target_slot) = first_empty_slot {
        let target = inventory.take(target_slot);
        let Some(mut target) = target else {
            return;
        };

        clicked.transfer(&mut target, i32::MAX);

        inventory.insert(target_slot, target);
    }
}