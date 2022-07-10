use std::marker::PhantomData;

use net::{packet_helper::PacketReadResult, network_handler::{ConnectionService, Connection}, network_buffer::WriteBuffer};

use crate::universe::{Universe, UniverseService};

pub struct PlayerConnection<U: UniverseService> {
    _phantom: PhantomData<U>
}

impl <U: UniverseService> PlayerConnection<U> {
    pub fn new() -> Self {
        Self {
            _phantom: PhantomData
        }
    }
}

impl <U: UniverseService> ConnectionService for PlayerConnection<U> {
    const BUFFER_SIZE: u32 = 4_194_304;
    type NetworkManagerServiceType = Universe<U>;

    fn on_receive(&mut self, connection: &mut Connection<Self::NetworkManagerServiceType>, num_bytes: u32) -> anyhow::Result<u32> {
        let mut bytes = connection.read_bytes(num_bytes);
        let mut write_buffer: WriteBuffer = WriteBuffer::new();

        loop {
            let packet_read_result = net::packet_helper::try_read_packet(&mut bytes)?;
            match packet_read_result {
                PacketReadResult::Complete(bytes) => {
                    println!("Request: {:x?}", bytes);
                    //self.process_framed_packet(&mut write_buffer, connection, bytes)?;
                }
                PacketReadResult::Partial => break,
                PacketReadResult::Empty => break,
            }
        }

        let bytes_remaining = bytes.len() as u32;

        let to_write = write_buffer.get_written();
        if to_write.len() > 0 {
            connection.write(to_write);
        }

        Ok(bytes_remaining)
    }
}


impl <U: UniverseService> PlayerConnection<U> {

}