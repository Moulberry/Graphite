use std::marker::PhantomData;

use binary::slice_serialization::{self, SliceSerializable};
use net::{
    network_buffer::WriteBuffer,
    network_handler::{Connection, ConnectionService},
    packet_helper::PacketReadResult,
};

use crate::universe::{Universe, UniverseService};

#[repr(C)]
pub struct PlayerConnection<U: UniverseService> {
    _phantom: PhantomData<U>,
    is_closing: bool,
    is_leaked: bool,
}

impl<U: UniverseService> PlayerConnection<U> {
    pub fn new() -> Self {
        Self {
            _phantom: PhantomData,
            is_closing: false,
            is_leaked: false,
        }
    }
}

impl<U: UniverseService> ConnectionService for PlayerConnection<U> {
    const BUFFER_SIZE: u32 = 4_194_304;
    type NetworkManagerServiceType = Universe<U>;

    fn on_receive(
        &mut self,
        connection: &mut Connection<Self::NetworkManagerServiceType>,
    ) -> anyhow::Result<u32> {
        let mut bytes = connection.read_bytes();
        let mut write_buffer: WriteBuffer = WriteBuffer::new();

        loop {
            let packet_read_result = net::packet_helper::try_read_packet(&mut bytes)?;
            match packet_read_result {
                PacketReadResult::Complete(bytes) => {
                    println!("Request: {:x?}", bytes);
                    self.process_framed_packet(&mut write_buffer, bytes)?;
                }
                PacketReadResult::Partial => break,
                PacketReadResult::Empty => break,
            }
        }

        let bytes_remaining = bytes.len() as u32;

        let to_write = write_buffer.get_written();
        if !to_write.is_empty() {
            connection.write(to_write);
        }

        Ok(bytes_remaining)
    }

    fn close(mut boxed: Box<Self>) {
        if !boxed.is_closing {
            boxed.is_closing = true;
            boxed.is_leaked = true;
            Box::leak(boxed);
        }
    }
}

impl<U: UniverseService> PlayerConnection<U> {
    pub(crate) fn check_connection_open(&mut self) -> bool {
        if self.is_closing {
            if self.is_leaked {
                unsafe { std::mem::drop(Box::from_raw(self)) };
            }

            false
        } else {
            true
        }
    }

    pub(crate) fn close_if_open(&mut self) -> bool {
        if self.is_closing {
            false
        } else {
            if self.is_leaked {
                unsafe { std::mem::drop(Box::from_raw(self)) };
            }
            self.is_closing = true;

            true
        }
    }

    fn process_framed_packet(
        &mut self,
        _: &mut WriteBuffer,
        // connection: &Connection<<PlayerConnection<U> as ConnectionService>::NetworkManagerServiceType>,
        bytes: &[u8],
    ) -> anyhow::Result<()> {
        let mut bytes = bytes;

        let packet_id_byte: u8 = slice_serialization::VarInt::read(&mut bytes)?.try_into()?;

        if let Ok(packet_id) = protocol::play::client::PacketId::try_from(packet_id_byte) {
            println!("got known packet id: {:?}", packet_id);
        } else {
            println!("unknown packet id: {:x}", packet_id_byte);
        }

        Ok(())
    }
}
