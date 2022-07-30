use binary::slice_serialization::SliceSerializable;
use net::{
    network_buffer::WriteBuffer,
    network_handler::ConnectionSlab,
    packet_helper::{self, PacketReadResult},
};
use protocol::{play, IdentifiedPacket};
use server::{
    player::{player_connection::AbstractConnectionReference, Player, PlayerService},
    universe::Universe,
};
use std::fmt::Debug;

use crate::log;

use super::{DummyPlayerService, DummyUniverseService};

pub struct FakePlayerConnection {
    player: *mut Player<DummyPlayerService>,
    handle_disconnect: fn(*mut ()),
    incoming_bytes: WriteBuffer,
    pub outgoing_bytes: WriteBuffer,
}

impl FakePlayerConnection {
    pub fn new() -> Self {
        Self {
            player: std::ptr::null_mut(),
            handle_disconnect: unsafe { std::mem::transmute(std::ptr::null_mut() as *mut ()) },
            incoming_bytes: WriteBuffer::new(),
            outgoing_bytes: WriteBuffer::new(),
        }
    }

    pub fn disconnect(&self) {
        (self.handle_disconnect)(self.player as *mut _);
    }

    pub fn assert_none_outgoing(&mut self) {
        let mut bytes = self.outgoing_bytes.get_written();
        let packet_bytes =
            packet_helper::try_read_packet(&mut bytes).expect("invalid packet was sent to player");
        match packet_bytes {
            PacketReadResult::Complete(packet_bytes) => {
                log!("Found packet with id: 0x{:x}", packet_bytes[0]);
                panic!(
                    "\npacket assertion failed: expected no more packets,\n\tgot: {}\n",
                    play::server::debug_print_packet(packet_bytes)
                )
            }
            PacketReadResult::Partial => panic!("packet was only partially written"),
            PacketReadResult::Empty => {}
        }
    }

    pub fn skip_all_outgoing(&mut self) {
        println!("self has ptr: {:?}", self as *mut _);
        self.outgoing_bytes.reset();
    }

    pub fn skip_outgoing(&mut self, packet_id: u8) {
        let bytes = self.outgoing_bytes.get_written().to_owned();
        let mut bytes: &[u8] = &bytes;

        let packet_bytes =
            packet_helper::try_read_packet(&mut bytes).expect("invalid packet was sent to player");

        // Remove the packet from the buffer
        self.outgoing_bytes.reset();
        self.outgoing_bytes.copy_from(&bytes);

        match packet_bytes {
            PacketReadResult::Complete(packet_bytes) => {
                assert_eq!(
                    packet_id, packet_bytes[0],
                    "expected: 0x{:x}, found: 0x{:x}",
                    packet_id, packet_bytes[0]
                );
            }
            PacketReadResult::Partial => panic!("packet was only partially written"),
            PacketReadResult::Empty => panic!("expected a packet, but there was none"),
        }
    }

    pub fn assert_outgoing_as<'a, T, F>(&mut self, func: F)
    where
        T: Debug + SliceSerializable<'a, T> + IdentifiedPacket<play::server::PacketId> + 'a,
        F: FnOnce(&mut T),
    {
        let bytes = self.outgoing_bytes.get_written().to_owned();
        let mut bytes: &[u8] = &bytes;

        let packet_bytes =
            packet_helper::try_read_packet(&mut bytes).expect("invalid packet was sent to player");

        // Remove the packet from the buffer
        self.outgoing_bytes.reset();
        self.outgoing_bytes.copy_from(&bytes);

        match packet_bytes {
            PacketReadResult::Complete(mut packet_bytes) => {
                log!("Found packet with id: 0x{:x}", packet_bytes[0]);
                play::server::debug_handle_packet(&mut packet_bytes, func);
            }
            PacketReadResult::Partial => panic!("packet was only partially written"),
            PacketReadResult::Empty => panic!("expected a packet, but there was none"),
        }
    }

    pub fn assert_outgoing<'a, T>(&mut self, packet: &'a T)
    where
        T: Debug + SliceSerializable<'a, T> + IdentifiedPacket<play::server::PacketId> + 'a,
    {
        let bytes = self.outgoing_bytes.get_written().to_owned();
        let mut bytes: &[u8] = &bytes;

        let packet_bytes =
            packet_helper::try_read_packet(&mut bytes).expect("invalid packet was sent to player");

        // Remove the packet from the buffer
        self.outgoing_bytes.reset();
        self.outgoing_bytes.copy_from(&bytes);

        match packet_bytes {
            PacketReadResult::Complete(packet_bytes) => {
                log!("Found packet with id: 0x{:x}", packet_bytes[0]);

                let mut temp = WriteBuffer::new();
                if packet_helper::write_packet(&mut temp, packet).is_err() {
                    panic!("packet was too big");
                }
                let expected_bytes = &temp.get_written()[3..]; // remove the size header

                if packet_bytes == expected_bytes {
                    return; // Success!
                } else {
                    panic!(
                        "\npacket assertion failed!\n\texpected: {:?}\n\tgot: {}\n",
                        packet,
                        play::server::debug_print_packet(packet_bytes)
                    )
                }
            }
            PacketReadResult::Partial => panic!("packet was only partially written"),
            PacketReadResult::Empty => panic!("expected a packet, but there was none"),
        }
    }

    pub fn write_packet<'a, T>(&mut self, packet: &'a T) -> anyhow::Result<()>
    where
        T: SliceSerializable<'a, T> + IdentifiedPacket<play::client::PacketId> + 'a,
    {
        if packet_helper::write_packet(&mut self.incoming_bytes, packet).is_err() {
            panic!("packet was too big");
        }

        let bytes_remaining = Player::handle_packets(unsafe { &mut *self.player })?;
        assert_eq!(bytes_remaining, 0); // Player must have handled the entire packet
        Ok(())
    }
}

impl AbstractConnectionReference<DummyUniverseService> for *mut FakePlayerConnection {
    fn update_player_pointer<P: PlayerService>(&mut self, player: *mut Player<P>) {
        let conn = unsafe { &mut **self };
        conn.player = player as *mut _;

        // Set ptr to process_disconnection function
        let process_disconnect_ptr = Player::<P>::handle_disconnect as *const ();
        conn.handle_disconnect = unsafe { std::mem::transmute(process_disconnect_ptr) };
    }

    fn clear_player_pointer(&mut self) {
        // noop
    }

    fn read_bytes(&self) -> &[u8] {
        let conn = unsafe { &mut **self };
        conn.incoming_bytes.get_written()
    }

    fn write_bytes(&mut self, bytes: &[u8]) {
        let conn = unsafe { &mut **self };
        conn.outgoing_bytes.copy_from(bytes)
    }

    fn new_from_connection(_: &mut ConnectionSlab<Universe<DummyUniverseService>>, _: u16) -> Self {
        panic!("Cannot create a fake player from a real connection")
    }

    unsafe fn forget(&mut self) {
        // noop
    }
}
