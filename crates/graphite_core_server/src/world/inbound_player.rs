use std::{cell::{RefCell, UnsafeCell}, rc::Rc};

use graphite_binary::nbt::EncodedNBT;
use graphite_mc_protocol::{configuration::{self, serverbound::PacketHandler}, IdentifiedPacket};
use graphite_network::{Connection, FramedPacketHandler, HandleAction, PacketBuffer};

use crate::registry::{Registries, Registry};

use super::WorldExtension;

pub struct InboundPlayer<W: WorldExtension> {
    connection: Option<Rc<RefCell<Connection>>>,
    packet_buffer: PacketBuffer,

    join_data: Option<W::JoinData>,

    configuring_ticks: u16,
    finished_configuration: bool,
    sent_finish_configuration: bool
}
pub const MAX_CONFIGURE_TICKS: u16 = 20 * 30;

impl <W: WorldExtension> InboundPlayer<W> {
    pub fn new(join_data: W::JoinData, connection: Rc<RefCell<Connection>>) -> Rc<UnsafeCell<Self>> {
        let inbound_player = Self {
            connection: Some(connection.clone()),
            packet_buffer: PacketBuffer::new(),
            join_data: Some(join_data),
            configuring_ticks: 0,
            finished_configuration: false,
            sent_finish_configuration: false
        };
        let inbound_player = Rc::new(UnsafeCell::new(inbound_player));

        connection.borrow_mut().set_handler(inbound_player.clone());

        inbound_player
    }

    pub fn tick_should_remove(&mut self) -> bool {
        if self.join_data.is_none() {
            return true;
        }

        if let Some(connection) = &self.connection {
            if !self.packet_buffer.is_empty() {
                let mut connection = connection.borrow_mut();
                if connection.is_shutdown() {
                    return true;
                }
                connection.send(&mut self.packet_buffer);
            } else if connection.borrow().is_shutdown() {
                return true;
            }
        } else {
            return true;
        }

        self.configuring_ticks = self.configuring_ticks.saturating_add(1);
        self.configuring_ticks >= MAX_CONFIGURE_TICKS
    }

    pub fn send_registries(&mut self, registries: &Registries) {
        registries.chat_type.create_packet().write_packet(&mut self.packet_buffer);
        registries.dimension_type.create_packet().write_packet(&mut self.packet_buffer);
        registries.damage_type.create_packet().write_packet(&mut self.packet_buffer);
        registries.biomes.create_packet().write_packet(&mut self.packet_buffer);
        registries.painting_variant.create_packet().write_packet(&mut self.packet_buffer);
        registries.wolf_variant.create_packet().write_packet(&mut self.packet_buffer);
        registries.wolf_sound_variant.create_packet().write_packet(&mut self.packet_buffer);
        registries.cat_variant.create_packet().write_packet(&mut self.packet_buffer);
        registries.chicken_variant.create_packet().write_packet(&mut self.packet_buffer);
        registries.cow_variant.create_packet().write_packet(&mut self.packet_buffer);
        registries.frog_variant.create_packet().write_packet(&mut self.packet_buffer);
        registries.pig_variant.create_packet().write_packet(&mut self.packet_buffer);
    }

    pub fn send_finish(&mut self) {
        if self.sent_finish_configuration {
            return;
        }

        if let Some(connection) = &self.connection {
            configuration::clientbound::FinishConfiguration{}.write_packet(&mut self.packet_buffer);
            connection.borrow_mut().send(&mut self.packet_buffer);

            self.sent_finish_configuration = true;
        }
    }

    pub fn disconnect(&mut self, message: Option<EncodedNBT>) {
        if let Some(connection) = self.connection.take() {
            if let Some(message) = message {
                if self.sent_finish_configuration {
                    graphite_mc_protocol::play::clientbound::Disconnect {
                        message
                    }.write_packet(&mut self.packet_buffer);
                } else {
                    graphite_mc_protocol::configuration::clientbound::Disconnect {
                        message
                    }.write_packet(&mut self.packet_buffer);
                }

                connection.borrow_mut().send(&mut self.packet_buffer);
            }

            connection.borrow_mut().shutdown();
        }
    }

    pub fn try_end(&mut self) -> Option<(W::JoinData, Rc<RefCell<Connection>>)> {
        if self.finished_configuration {
            if let Some(data) = self.join_data.take() {
                if let Some(connection) = self.connection.take() {
                    connection.borrow_mut().disconnect_handler();
                    return Some((data, connection));
                }
            }
        }
        return None;
    }
}

impl <W: WorldExtension> Drop for InboundPlayer<W> {
    fn drop(&mut self) {
        self.disconnect(None);
    }
}

impl <W: WorldExtension> FramedPacketHandler for InboundPlayer<W> {
    fn handle(&mut self, data: &[u8]) -> HandleAction {
        match self.parse_and_handle(data, |_, _| false) {
            Ok(()) => HandleAction::Continue,
            Err(error) => if cfg!(debug_assertions) {
                panic!("Encountered error handling packet: {}", error);
            } else {
                HandleAction::Disconnect
            },
        }
    }

    fn disconnected(&mut self) {
        self.connection = None;
    }
}

impl <W: WorldExtension> graphite_mc_protocol::configuration::serverbound::PacketHandler for InboundPlayer<W> {
    fn handle_finish_configuration(&mut self, _: configuration::serverbound::FinishConfiguration) -> anyhow::Result<()> {
        self.finished_configuration = true;
        Ok(())
    }
}