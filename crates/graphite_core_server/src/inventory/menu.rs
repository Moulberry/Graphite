use std::{borrow::Cow, i32, marker::PhantomData, u128};

use anyhow::bail;
use enum_dispatch::enum_dispatch;
use graphite_binary::nbt::EncodedNBT;
use graphite_mc_constants::{builtin::ContainerType, types::{ClickType, EquipmentSlot}};
use graphite_mc_protocol::{play, types::ItemStack, IdentifiedPacket};
use graphite_network::PacketBuffer;
use num_enum::TryFromPrimitive;

use crate::player::{Player, PlayerExtension};

use super::{inventory_slot::InventorySlot, item_holder::{ItemHolder, ItemHolderRef}, player_container_view::PlayerFunc};

#[derive(Debug, Clone)]
pub enum ContainerAction {
    Click {
        slot: InventorySlot,
        shift_down: bool,
        right_click: bool
    },
    Swap(InventorySlot, InventorySlot),
    Drag {
        slots: Box<[InventorySlot]>,
        right_click: bool
    },
    PickupAll,
    Clone(InventorySlot),
    Throw {
        slot: InventorySlot,
        all: bool,
        outside: bool
    },
    SelectTrade(usize),
    SelectBundleItem {
        slot: InventorySlot,
        selected: i32
    }
}

#[derive(Copy, Clone, PartialEq)]
pub enum ShouldReopen {
    Yes,
    No
}

#[enum_dispatch]
#[allow(unused)]
pub trait BaseMenu<P: PlayerExtension> {
    fn container_type(&self) -> ContainerType;
    fn title(&self) -> EncodedNBT;
    fn send_extra_open_packets(&self, container_id: i32, packet_buffer: &mut PacketBuffer);
    fn get_item_stack(&self, inventory: &P::InventoryContainer, slot: InventorySlot) -> ItemStack;
    fn do_container_action(&mut self, inventory: &mut P::InventoryContainer, action: ContainerAction) -> PlayerFunc<P>;
    fn closed(self, inventory: &mut P::InventoryContainer, by_player: bool) -> PlayerFunc<P>;
    fn synchronize_tick(&mut self, container_id: i32, packet_buffer: &mut graphite_network::PacketBuffer) -> ShouldReopen {
        ShouldReopen::No
    }
    fn on_synchronize_slot(&mut self, packet_buffer: &mut PacketBuffer, slot: InventorySlot) {
    }
}

pub trait ContainerMenu<P: PlayerExtension>: BaseMenu<P> {
    type Item: ItemHolder;

    fn take(&mut self, inventory: &mut P::InventoryContainer, slot: InventorySlot) -> Option<Self::Item>;
    fn insert(&mut self, inventory: &mut P::InventoryContainer, slot: InventorySlot, item: Self::Item);
    fn is_item_compatible_with(&self, inventory: &P::InventoryContainer, item: &Self::Item, slot: InventorySlot) -> bool;

    fn handle_container_action(&mut self, inventory: &mut P::InventoryContainer, action: ContainerAction) {
        match action {
            ContainerAction::Click { slot: clicked_slot, shift_down, right_click } => {
                if shift_down {
                    let Some(mut clicked) = self.take(inventory, clicked_slot) else {
                        return;
                    };

                    let container_type = self.container_type();

                    if container_type == ContainerType::Crafting {
                        match clicked_slot {
                            InventorySlot::Container(0) => { 
                                shift_move_items(self, inventory, &mut clicked, InventorySlot::main_and_hotbar().rev());
                            },
                            InventorySlot::Container(_) => { 
                                shift_move_items(self, inventory, &mut clicked, InventorySlot::main_and_hotbar());
                            },
                            InventorySlot::Hotbar(_) => {
                                let crafting = (1..10).map(|i| InventorySlot::Container(i));
                                shift_move_items(self, inventory, &mut clicked, crafting);
                                if clicked.visual_count() > 0 {
                                    shift_move_items(self, inventory, &mut clicked, InventorySlot::all_main());
                                }
                            },
                            InventorySlot::Main(_) => {
                                let crafting = (1..10).map(|i| InventorySlot::Container(i));
                                shift_move_items(self, inventory, &mut clicked, crafting);
                                if clicked.visual_count() > 0 {
                                    shift_move_items(self, inventory, &mut clicked, InventorySlot::all_hotbar());
                                }
                            }
                            _ => {}
                        }
                    } else {
                        match clicked_slot {
                            InventorySlot::Container(_) => {
                                shift_move_items(self, inventory, &mut clicked, InventorySlot::main_and_hotbar().rev());
                            },
                            InventorySlot::Main(_) | InventorySlot::Hotbar(_) => {
                                let size = self.container_type().slot_count();
    
                                if size > 36 {
                                    let slots = (0..size-36).map(|i| InventorySlot::Container(i as u8));
                                    shift_move_items(self, inventory, &mut clicked, slots);
                                }
                            }
                            _ => {}
                        }
                    }
    
                    self.insert(inventory, clicked_slot, clicked);
                } else {
                    let Some(mut clicked) = self.take(inventory, clicked_slot) else {
                        return;
                    };
    
                    if !self.is_item_compatible_with(inventory, &clicked, InventorySlot::Carried) {
                        self.insert(inventory, clicked_slot, clicked);
                        return;
                    }
    
                    let Some(mut carried) = self.take(inventory, InventorySlot::Carried) else {
                        self.insert(inventory, clicked_slot, clicked);
                        return;
                    };
    
                    if !self.is_item_compatible_with(inventory, &carried, clicked_slot) {
                        self.insert(inventory, clicked_slot, clicked);
                        self.insert(inventory, InventorySlot::Carried, carried);
                        return;
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
    
                    self.insert(inventory, clicked_slot, clicked);
                    self.insert(inventory, InventorySlot::Carried, carried);
                }
            },
            ContainerAction::Swap(from_slot, to_slot) => {
                let Some(mut from) = self.take(inventory, from_slot) else {
                    return;
                };
    
                if !self.is_item_compatible_with(inventory, &from, to_slot) {
                    self.insert(inventory, from_slot, from);
                    return;
                }
    
                let Some(mut to) = self.take(inventory, to_slot) else {
                    self.insert(inventory, from_slot, from);
                    return;
                };
    
                if self.is_item_compatible_with(inventory, &to, from_slot) {
                    std::mem::swap(&mut from, &mut to);
                }
    
                self.insert(inventory, from_slot, from);
                self.insert(inventory, to_slot, to);
            },
            ContainerAction::Drag { slots, right_click } => {
                if slots.is_empty() {
                    return;
                }
    
                let Some(mut carried) = self.take(inventory, InventorySlot::Carried) else {
                    return;
                };
                
                let mut transfer_count = carried.visual_count() / slots.len() as i32;
                if right_click {
                    transfer_count = transfer_count.min(1);
                }
    
                if transfer_count > 0 {
                    for &slot in slots.iter() {
                        if !self.is_item_compatible_with(inventory, &mut carried, slot) {
                            continue;
                        }
        
                        let Some(mut item) = self.take(inventory, slot) else {
                            continue;
                        };
        
                        carried.transfer(&mut item, transfer_count);
        
                        self.insert(inventory, slot, item);
                    }
                }
    
                self.insert(inventory, InventorySlot::Carried, carried);
            },
            ContainerAction::PickupAll => {
                let Some(mut carried) = self.take(inventory, InventorySlot::Carried) else {
                    return;
                };

                if carried.visual_count() <= 0 {
                    self.insert(inventory, InventorySlot::Carried, carried);
                    return;
                }
    
                for &slot in InventorySlot::all_for_container(Some(self.container_type())) {
                    if !self.is_item_compatible_with(inventory, &mut carried, slot) {
                        continue;
                    }
    
                    let Some(mut item) = self.take(inventory, slot) else {
                        continue;
                    };
    
                    item.transfer(&mut carried, i32::MAX);
    
                    self.insert(inventory, slot, item);
                }
    
                self.insert(inventory, InventorySlot::Carried, carried);
            },
            ContainerAction::Clone(_) => {},
            ContainerAction::Throw { slot: _, all: _, outside: _, } => {},
            ContainerAction::SelectTrade(_) => {},
            ContainerAction::SelectBundleItem { slot: _, selected: _ } => {}
        }
    }
}

fn shift_move_items<P: PlayerExtension, C: ContainerMenu<P> + ?Sized>(menu: &mut C, inventory: &mut P::InventoryContainer,
        clicked: &mut C::Item, slots: impl DoubleEndedIterator<Item = InventorySlot>) {
    let mut first_empty_slot = None;

    for target_slot in slots {
        if !menu.is_item_compatible_with(inventory, &clicked, target_slot) {
            continue;
        }

        let target = menu.take(inventory, target_slot);
        let Some(mut target) = target else {
            continue;
        };

        let visual_count = target.visual_count();
        if visual_count > 0 {
            clicked.transfer(&mut target, i32::MAX);
            if clicked.visual_count() <= 0 {
                menu.insert(inventory, target_slot, target);
                return;
            }
        } else if first_empty_slot.is_none() {
            first_empty_slot = Some(target_slot);
        }

        menu.insert(inventory, target_slot, target);
    }

    if let Some(target_slot) = first_empty_slot {
        let target = menu.take(inventory, target_slot);
        let Some(mut target) = target else {
            return;
        };

        clicked.transfer(&mut target, i32::MAX);

        menu.insert(inventory, target_slot, target);
    }
}