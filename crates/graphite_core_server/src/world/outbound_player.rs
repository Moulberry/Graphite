use std::{cell::{RefCell, UnsafeCell}, rc::Rc};

use graphite_binary::nbt::EncodedNBT;
use graphite_mc_protocol::{play::serverbound::PacketHandler, IdentifiedPacket};
use graphite_network::{Connection, FramedPacketHandler, HandleAction, PacketBuffer, SendableConnection};

pub struct OutboundPlayer {
    connection: Option<Rc<RefCell<Connection>>>,
    packet_buffer: PacketBuffer,
    transfer: Option<Box<dyn FnOnce(SendableConnection)>>,
    waiting_ticks: u16
}
pub const MAX_WAITING_TICKS: u16 = 20 * 5;

impl OutboundPlayer {
    pub fn new(connection: Rc<RefCell<Connection>>, packet_buffer: PacketBuffer, transfer: Box<dyn FnOnce(SendableConnection)>) -> Rc<UnsafeCell<Self>> {
        let outbound_player = Self {
            connection: Some(connection.clone()),
            packet_buffer,
            transfer: Some(transfer),
            waiting_ticks: 0
        };
        let outbound_player = Rc::new(UnsafeCell::new(outbound_player));

        connection.borrow_mut().set_handler(outbound_player.clone());

        outbound_player
    }

    pub fn tick_should_remove(&mut self) -> bool {
        if self.transfer.is_none() {
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

        self.waiting_ticks = self.waiting_ticks.saturating_add(1);
        self.waiting_ticks >= MAX_WAITING_TICKS
    }

    pub fn disconnect(&mut self, message: Option<EncodedNBT>) {
        if let Some(connection) = self.connection.take() {
            if let Some(message) = message {
                graphite_mc_protocol::configuration::clientbound::Disconnect {
                    message
                }.write_packet(&mut self.packet_buffer);
                connection.borrow_mut().send(&mut self.packet_buffer);
            }

            connection.borrow_mut().shutdown();
        }
    }
}

impl Drop for OutboundPlayer {
    fn drop(&mut self) {
        self.disconnect(None);
    }
}

impl FramedPacketHandler for OutboundPlayer {
    fn handle(&mut self, data: &[u8]) -> HandleAction {
        match self.parse_and_handle(data, |_, _| false) {
            Ok(ReadyToTransfer::No) => HandleAction::Continue,
            Ok(ReadyToTransfer::Yes) => {
                let Some(transfer) = self.transfer.take() else {
                    return HandleAction::Disconnect;
                };

                HandleAction::Transfer(transfer)
            },
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

#[derive(Default)]
enum ReadyToTransfer {
    Yes,
    #[default]
    No
}

impl graphite_mc_protocol::play::serverbound::PacketHandler<ReadyToTransfer> for OutboundPlayer {
    fn handle_acknowledge_configuration(&mut self, _: graphite_mc_protocol::play::serverbound::AcknowledgeConfiguration) -> anyhow::Result<ReadyToTransfer> {
        Ok(ReadyToTransfer::Yes)
    }
}