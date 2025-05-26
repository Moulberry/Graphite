use std::{borrow::Cow, i32, marker::PhantomData, pin::Pin, u128};

use anyhow::bail;
use graphite_binary::nbt::EncodedNBT;
use graphite_mc_constants::{builtin::ContainerType, types::ClickType};
use graphite_mc_protocol::{play, types::{hashed_stack::HashedStack, ItemStack}, IdentifiedPacket};
use graphite_network::PacketBuffer;
use num_enum::TryFromPrimitive;

use crate::{inventory::container, player::{DropCause, Player, PlayerExtension}};

use super::{container::Container, inventory_slot::InventorySlot, item_holder::{ItemHolder, ItemHolderRef}, menu::{BaseMenu, ContainerAction, ShouldReopen}};

pub type PlayerFunc<P> = Option<Box<dyn FnOnce(&mut Player<P>)>>;

#[derive(PartialEq, Eq, TryFromPrimitive)]
#[repr(i8)]
enum MouseDragStatus {
    Starting = 0,
    Adding = 1,
    End = 2
}

#[derive(PartialEq, Eq, TryFromPrimitive)]
#[repr(i8)]
enum MouseDragType {
    Left = 0,
    Right = 1,
    Middle = 2,
}

struct MenuInfo<P: PlayerExtension> {
    menu: P::Menu,
    container_id: i32,
    container_type: ContainerType,
    title: EncodedNBT,
}

#[derive(Clone)]
enum RemoteStack {
    Empty,
    HashedStack(HashedStack),
    ItemStack(ItemStack),
}

impl RemoteStack {
    fn matches(&self, item_stack: &ItemStack) -> bool {
        match self {
            RemoteStack::Empty => item_stack.is_empty(),
            RemoteStack::HashedStack(hashed_stack) => hashed_stack.hashed_equals(item_stack),
            RemoteStack::ItemStack(remote_stack) => remote_stack == item_stack,
        }
    }
}

pub struct PlayerContainerView<P: PlayerExtension> {
    pub inventory: P::InventoryContainer,
    open_menu: Option<MenuInfo<P>>,

    last_used_container_id: i32,
    state_id: i32,
    last_remote_state_id: i32,

    remote_item_stacks: Vec<RemoteStack>,
    force_synchronize: u128,
    synchronize_offhand: bool,

    last_mouse_drag_status: MouseDragStatus,
    mouse_drag_type: MouseDragType,
    mouse_drag_slots: u128,
    mouse_drag_slots_ordered: Vec<u8>
}

impl <P: PlayerExtension> PlayerContainerView<P> {
    pub fn new(inventory: P::InventoryContainer) -> Self {
        Self {
            inventory,
            open_menu: None,

            last_used_container_id: 0,
            state_id: 0,
            last_remote_state_id: 0,

            remote_item_stacks: Vec::new(),
            force_synchronize: u128::MAX,
            synchronize_offhand: false,

            last_mouse_drag_status: MouseDragStatus::Starting,
            mouse_drag_type: MouseDragType::Left,
            mouse_drag_slots: 0,
            mouse_drag_slots_ordered: Vec::new()
        }
    }

    pub fn get_container_type(&self) -> Option<ContainerType> {
        self.open_menu.as_ref().map(|open_menu| open_menu.container_type)
    }

    pub fn get_container_id(&self) -> i32 {
        match self.open_menu.as_ref() {
            Some(open_menu) => open_menu.container_id,
            None => 0,
        }
    }

    pub fn get_menu(&self) -> Option<&P::Menu> {
        self.open_menu.as_ref().map(|open_menu| &open_menu.menu)
    }

    pub fn get_menu_mut(&mut self) -> Option<&mut P::Menu> {
        self.open_menu.as_mut().map(|open_menu| &mut open_menu.menu)
    }

    pub fn get_menu_and_inventory_mut(&mut self) -> Option<(&mut P::Menu, &mut P::InventoryContainer)> {
        self.open_menu.as_mut().map(|open_menu| (&mut open_menu.menu, &mut self.inventory))
    }

    pub(crate) fn open_menu(&mut self, menu: P::Menu, packet_buffer: &mut PacketBuffer) {
        if self.open_menu.is_some() {
            panic!("New menu was opened before old menu was closed");
        }

        self.last_mouse_drag_status = MouseDragStatus::Starting;
        self.mouse_drag_slots = 0;
        self.mouse_drag_slots_ordered.clear();

        self.last_used_container_id = self.last_used_container_id.wrapping_add(1);
        if self.last_used_container_id < 1 {
            self.last_used_container_id = 1;
        }

        let container_id = self.last_used_container_id;
        let container_type = menu.container_type();
        let title = menu.title();

        play::clientbound::OpenScreen {
            container_id,
            screen_type: container_type as i32,
            title: title.clone(),
        }.write_packet(packet_buffer);

        self.state_id = 0;
        self.open_menu = Some(MenuInfo {
            menu,
            container_id,
            container_type,
            title,
        });

        self.synchronize_fully(packet_buffer);

        let open_menu = self.open_menu.as_ref().expect("menu is open");
        open_menu.menu.send_extra_open_packets(container_id, packet_buffer);
    }

    pub(crate) fn drop_held_item(&mut self) -> Option<<<P as PlayerExtension>::InventoryContainer as Container>::Item> {
        let Some(mut item) = self.inventory.take(InventorySlot::Carried) else {
            return None;
        };

        if item.visual_count() <= 0 {
            self.inventory.insert(InventorySlot::Carried, item);
            return None;
        }

        let mut thrown = item.clone_empty();
        item.transfer(&mut thrown, i32::MAX);

        self.inventory.insert(InventorySlot::Carried, item);

        return Some(thrown);
    }

    pub(crate) fn close(&mut self, packet_buffer: &mut PacketBuffer, send_close: bool) -> PlayerFunc<P> {
        if let Some(open_menu) = self.open_menu.take() {
            if send_close {
                play::clientbound::ContainerClose {
                    container_id: open_menu.container_id,
                }.write_packet(packet_buffer);
            }

            return open_menu.menu.closed(&mut self.inventory, false);
        }
        None
    }

    pub(crate) fn do_container_close(&mut self, container_close: play::serverbound::ContainerClose) -> PlayerFunc<P> {
        if let Some(open_menu) = &self.open_menu {
            if container_close.container_id == open_menu.container_id {
                let open_menu = self.open_menu.take().expect("has menu open");
                return open_menu.menu.closed(&mut self.inventory, true);
            }
        }
        None
    }

    pub(crate) fn do_container_action(&mut self, action: ContainerAction) -> PlayerFunc<P> {
        if let Some(open_menu) = self.open_menu.as_mut() {
            open_menu.menu.do_container_action(&mut self.inventory, action)
        } else {
            container::handle_container_action_for_inventory::<P>(&mut self.inventory, action)
        }
    }

    pub fn do_select_trade(&mut self, select_trade: play::serverbound::SelectTrade) -> anyhow::Result<PlayerFunc<P>> {
        let trade: usize = select_trade.trade.try_into()?;
        return Ok(self.do_container_action(ContainerAction::SelectTrade(trade)));
    }

    pub fn do_bundle_item_selected(&mut self, bundle_item_selected: play::serverbound::BundleItemSelected) -> anyhow::Result<PlayerFunc<P>> {
        let container_type = self.get_container_type();
        let slot = InventorySlot::from_index(bundle_item_selected.slot as i16, container_type)?;
        
        return Ok(self.do_container_action(ContainerAction::SelectBundleItem {
            slot,
            selected: bundle_item_selected.selected_index
        }));
    }

    pub fn get_inventory_item_stack(&self, slot: InventorySlot) -> ItemStack {
        self.inventory.get_item_stack(slot).unwrap_or_default()
    }

    pub fn force_synchronize_slot(&mut self, slot: InventorySlot) {
        if let Ok(index) = slot.get_index(self.get_container_type()) {
            self.force_synchronize |= 1 << index;
        }
    }

    pub fn force_synchronize_all(&mut self) {
        self.force_synchronize |= u128::MAX;
    }

    pub(crate) fn get_container_action_from_click(&mut self, container_click: play::serverbound::ContainerClick) -> anyhow::Result<Option<ContainerAction>> {
        if container_click.container_id != self.get_container_id() || container_click.state_id < self.last_remote_state_id {
            return Ok(None);
        }

        self.last_remote_state_id = container_click.state_id;

        if container_click.state_id < self.state_id {
            self.force_synchronize = u128::MAX;
        } else if self.force_synchronize != u128::MAX && !self.remote_item_stacks.is_empty() {
            // Update remote slots
            for changed_slot in container_click.changed_slots {
                if changed_slot.slot < self.remote_item_stacks.len() as i16 {
                    self.remote_item_stacks[changed_slot.slot as usize] = changed_slot.item
                        .map_or(RemoteStack::Empty, |v| RemoteStack::HashedStack(v));
                }
            }

            // Update remote carried
            let carried_index = self.remote_item_stacks.len() - 1;
            self.remote_item_stacks[carried_index] = container_click.carried_item
                .map_or(RemoteStack::Empty, |v| RemoteStack::HashedStack(v));
        }

        let slot = container_click.slot;
        let button = container_click.button;
        let mode = container_click.mode;
        let container_type = self.get_container_type();

        if mode != ClickType::QuickCraft && self.last_mouse_drag_status != MouseDragStatus::Starting {
            self.last_mouse_drag_status = MouseDragStatus::Starting;
            self.mouse_drag_slots = 0;
            self.mouse_drag_slots_ordered.clear();
        }

        match container_click.mode {
            ClickType::Pickup => {
                if button != 0 && button != 1 {
                    bail!("Invalid button {}", button);
                }

                if slot == -999 {
                    return Ok(Some(ContainerAction::Throw {
                        slot: InventorySlot::Carried,
                        all: button == 0,
                        outside: true,
                    }));
                } else if slot == -1 {
                    return Ok(None);
                }

                let clicked_slot = InventorySlot::from_index(slot, container_type)?;
                if clicked_slot == InventorySlot::Carried {
                    bail!("Can't click on Carried");
                }

                return Ok(Some(ContainerAction::Click {
                    slot: clicked_slot,
                    shift_down: false,
                    right_click: button == 1
                }));
            },
            ClickType::QuickMove => { // aka shift click
                let quick_move_slot = InventorySlot::from_index(slot, container_type)?;
                return Ok(Some(ContainerAction::Click {
                    slot: quick_move_slot,
                    shift_down: true,
                    right_click: button == 1
                }));
            },
            ClickType::Swap => {
                let swap_from_slot = InventorySlot::from_index(slot, container_type)?;
                if swap_from_slot == InventorySlot::Carried {
                    bail!("Can't swap from Carried");
                }

                let swap_to_slot = match button {
                    0 => InventorySlot::Hotbar(0),
                    1 => InventorySlot::Hotbar(1),
                    2 => InventorySlot::Hotbar(2),
                    3 => InventorySlot::Hotbar(3),
                    4 => InventorySlot::Hotbar(4),
                    5 => InventorySlot::Hotbar(5),
                    6 => InventorySlot::Hotbar(6),
                    7 => InventorySlot::Hotbar(7),
                    8 => InventorySlot::Hotbar(8),
                    40 => InventorySlot::OffHand,
                    _ => bail!("invalid swap button: {}", button)
                };

                if swap_from_slot == swap_to_slot {
                    return Ok(None);
                } else {
                    if swap_to_slot == InventorySlot::OffHand {
                        self.synchronize_offhand = true;
                    }
                    return Ok(Some(ContainerAction::Swap(swap_from_slot, swap_to_slot)));
                }
            },
            ClickType::Clone => {
                let clone_slot = InventorySlot::from_index(slot, container_type)?;
                return Ok(Some(ContainerAction::Clone(clone_slot)));
            },
            ClickType::Throw => {
                if slot == -999 {
                    return Ok(Some(ContainerAction::Throw {
                        slot: InventorySlot::Carried,
                        all: button == 1,
                        outside: true
                    }));
                }
                let throw_slot = InventorySlot::from_index(slot, container_type)?;
                return Ok(Some(ContainerAction::Throw {
                    slot: throw_slot,
                    all: button == 1,
                    outside: false
                }));
            },
            ClickType::QuickCraft => { // aka drag click
                let mouse_drag_status: MouseDragStatus = (button & 3).try_into()?;

                if mouse_drag_status == self.last_mouse_drag_status {
                    if mouse_drag_status == MouseDragStatus::Starting {
                        // Initiate drag
                        self.mouse_drag_type = ((button >> 2) & 3).try_into()?;

                        if self.mouse_drag_type == MouseDragType::Middle {
                            self.mouse_drag_type = MouseDragType::Left;
                        }

                        self.last_mouse_drag_status = MouseDragStatus::Adding;
                        self.mouse_drag_slots = 0;
                        self.mouse_drag_slots_ordered.clear();

                        return Ok(None);
                    } else if mouse_drag_status == MouseDragStatus::Adding {
                        // Add more slots to drag
                        if slot >= u8::MIN as i16 && slot <= u8::MAX as i16 {
                            InventorySlot::from_index(slot, container_type)?;
    
                            if (self.mouse_drag_slots & (1 << slot)) == 0 {
                                self.mouse_drag_slots |= 1 << slot;
                                self.mouse_drag_slots_ordered.push(slot as u8);
                            }
                        }

                        return Ok(None);
                    }
                } else if self.last_mouse_drag_status == MouseDragStatus::Adding && mouse_drag_status == MouseDragStatus::End {
                    // Finish drag
                    let button = match self.mouse_drag_type {
                        MouseDragType::Left => 0,
                        MouseDragType::Right => 1,
                        MouseDragType::Middle => 0,
                    };

                    let drag_count = self.mouse_drag_slots_ordered.len();
                    if drag_count == 1 {
                        if let Some(slot) = self.mouse_drag_slots_ordered.last() {
                            let clicked_slot = InventorySlot::from_index(*slot as i16, container_type)?;
    
                            self.last_mouse_drag_status = MouseDragStatus::Starting;
                            self.mouse_drag_slots = 0;
                            self.mouse_drag_slots_ordered.clear();
    
                            return Ok(Some(ContainerAction::Click {
                                slot: clicked_slot,
                                shift_down: false,
                                right_click: button == 1
                            }));
                        }
                    } else if drag_count > 1 {
                        let slots = self.mouse_drag_slots_ordered.iter().filter_map(|&slot| {
                            InventorySlot::from_index(slot as i16, container_type).ok()
                        }).collect();
                        
                        self.last_mouse_drag_status = MouseDragStatus::Starting;
                        self.mouse_drag_slots = 0;
                        self.mouse_drag_slots_ordered.clear();

                        return Ok(Some(ContainerAction::Drag {
                            slots,
                            right_click: button == 1
                        }));
                    }
                }

                self.last_mouse_drag_status = MouseDragStatus::Starting;
                self.mouse_drag_slots = 0;
                self.mouse_drag_slots_ordered.clear();

                return Ok(None);
            },
            ClickType::PickupAll => {
                return Ok(Some(ContainerAction::PickupAll));
            },
        }
    }

    pub fn synchronize(&mut self, packet_buffer: &mut PacketBuffer) {
        if let Some(open_menu) = self.open_menu.as_mut() {
            // Call synchronize tick and reopen menu if requested
            let should_reopen = open_menu.menu.synchronize_tick(open_menu.container_id, packet_buffer);
            if should_reopen == ShouldReopen::Yes {
                let title = open_menu.menu.title();
                let container_type = open_menu.menu.container_type();

                open_menu.title = title.clone();
                open_menu.container_type = container_type;

                play::clientbound::OpenScreen {
                    container_id: open_menu.container_id,
                    screen_type: open_menu.container_type as i32,
                    title,
                }.write_packet(packet_buffer);

                self.synchronize_fully(packet_buffer);

                let open_menu = self.open_menu.as_ref().expect("menu is open");
                open_menu.menu.send_extra_open_packets(open_menu.container_id, packet_buffer);
                return;
            }

            if self.force_synchronize == u128::MAX {
                self.synchronize_fully(packet_buffer);
                return;
            }

            let all_slots = InventorySlot::all_for_container(Some(open_menu.container_type));

            if self.remote_item_stacks.len() != all_slots.len() + 1 {
                self.synchronize_fully(packet_buffer);
                return;
            }

            // 1. Check if force_synchronize
            // 2. Compare RemoteStack
            /*
            enum RemoteStack {
                HashedStack(HashedStack),
                ItemStack(ItemStack),
            }
             */


            // Sync main inventory slots
            for (index, &slot) in all_slots.iter().enumerate() {
                let item_stack = open_menu.menu.get_item_stack(&mut self.inventory, slot);

                if self.force_synchronize & (1 << index) != 0 || !self.remote_item_stacks[index].matches(&item_stack) {
                    self.state_id = self.state_id.wrapping_add(1);
                    graphite_mc_protocol::play::clientbound::ContainerSetSlot {
                        container_id: open_menu.container_id,
                        state_id: self.state_id,
                        slot: index as i16,
                        item: item_stack.clone(),
                    }.write_packet(packet_buffer);
                    P::Menu::on_synchronize_slot(&mut open_menu.menu, packet_buffer, slot);
                }
                self.remote_item_stacks[index] = RemoteStack::ItemStack(item_stack);
            }

            // Sync carried slot
            let carried = open_menu.menu.get_item_stack(&mut self.inventory, InventorySlot::Carried);
            let carried_index = self.remote_item_stacks.len() - 1;
            if self.force_synchronize & (1 << carried_index) != 0 || !self.remote_item_stacks[carried_index].matches(&carried) {
                graphite_mc_protocol::play::clientbound::SetCursorItem {
                    item: carried.clone(),
                }.write_packet(packet_buffer);
                P::Menu::on_synchronize_slot(&mut open_menu.menu, packet_buffer, InventorySlot::Carried);
            }
            self.remote_item_stacks[carried_index] = RemoteStack::ItemStack(carried);

            // Sync off-hand if needed
            if self.synchronize_offhand {
                self.synchronize_offhand = false;
                let index = InventorySlot::OffHand.get_index(None).unwrap();
                graphite_mc_protocol::play::clientbound::ContainerSetSlot {
                    container_id: 0,
                    state_id: 0,
                    slot: index as i16,
                    item: self.inventory.get_item_stack(InventorySlot::OffHand).unwrap_or_default(),
                }.write_packet(packet_buffer);
                P::Menu::on_synchronize_slot(&mut open_menu.menu, packet_buffer, InventorySlot::OffHand);
            }
        } else {
            if self.force_synchronize == u128::MAX {
                self.synchronize_fully(packet_buffer);
                return;
            }

            let all_slots = InventorySlot::all_for_container(None);

            if self.remote_item_stacks.len() != all_slots.len() + 1 {
                self.synchronize_fully(packet_buffer);
                return;
            }

            // Sync main inventory slots
            for (index, &slot) in all_slots.iter().enumerate() {
                let item_stack = self.get_inventory_item_stack(slot);

                if self.force_synchronize & (1 << index) != 0 || !self.remote_item_stacks[index].matches(&item_stack) {
                    self.state_id = self.state_id.wrapping_add(1);
                    graphite_mc_protocol::play::clientbound::ContainerSetSlot {
                        container_id: 0,
                        state_id: self.state_id,
                        slot: index as i16,
                        item: item_stack.clone(),
                    }.write_packet(packet_buffer);
                }
                self.remote_item_stacks[index] = RemoteStack::ItemStack(item_stack);
            }

            // Sync carried slot
            let carried = self.get_inventory_item_stack(InventorySlot::Carried);
            let carried_index = self.remote_item_stacks.len() - 1;
            if self.force_synchronize & (1 << carried_index) != 0 || !self.remote_item_stacks[carried_index].matches(&carried) {
                graphite_mc_protocol::play::clientbound::SetCursorItem {
                    item: carried.clone(),
                }.write_packet(packet_buffer);
            }
            self.remote_item_stacks[carried_index] = RemoteStack::ItemStack(carried);
        }
    }

    fn synchronize_fully(&mut self, packet_buffer: &mut PacketBuffer) {
        self.force_synchronize = 0;
        self.synchronize_offhand = false;

        if let Some(open_menu) = self.open_menu.as_mut() {
            let container_type = open_menu.container_type;
            let all_slots = InventorySlot::all_for_container(Some(container_type));
    
            self.remote_item_stacks.resize(all_slots.len() + 1, RemoteStack::Empty);
            let mut all_items = Vec::with_capacity(all_slots.len());
    
            for (index, &slot) in all_slots.iter().enumerate() {
                let item_stack = open_menu.menu.get_item_stack(&mut self.inventory, slot);
                self.remote_item_stacks[index] = RemoteStack::ItemStack(item_stack.clone());
                all_items.push(item_stack);
            }
    
            let carried = open_menu.menu.get_item_stack(&mut self.inventory, InventorySlot::Carried);
            let carried_index = self.remote_item_stacks.len() - 1;
            self.remote_item_stacks[carried_index] = RemoteStack::ItemStack(carried.clone());
    
            self.state_id = self.state_id.wrapping_add(1);
            self.last_remote_state_id = self.state_id;
    
            play::clientbound::ContainerSetContent {
                container_id: open_menu.container_id,
                state_id: self.state_id,
                slots: Cow::Owned(all_items),
                carried
            }.write_packet(packet_buffer);
        } else {
            let all_slots = InventorySlot::all_for_container(None);
    
            self.remote_item_stacks.resize(all_slots.len() + 1, RemoteStack::Empty);
            let mut all_items = Vec::with_capacity(all_slots.len());
    
            for (index, &slot) in all_slots.iter().enumerate() {
                let item_stack = self.get_inventory_item_stack(slot);
                self.remote_item_stacks[index] = RemoteStack::ItemStack(item_stack.clone());
                all_items.push(item_stack);
            }
    
            let carried = self.get_inventory_item_stack(InventorySlot::Carried);
            let carried_index = self.remote_item_stacks.len() - 1;
            self.remote_item_stacks[carried_index] = RemoteStack::ItemStack(carried.clone());
    
            self.state_id = self.state_id.wrapping_add(1);
            self.last_remote_state_id = self.state_id;
    
            play::clientbound::ContainerSetContent {
                container_id: 0,
                state_id: self.state_id,
                slots: Cow::Owned(all_items),
                carried
            }.write_packet(packet_buffer);
        }

    }
}


