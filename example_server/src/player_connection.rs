use std::marker::PhantomData;

use anyhow::bail;
use binary::slice_serialization::{self, SliceSerializable};
use net::{
    network_buffer::WriteBuffer,
    network_handler::{Connection, ConnectionService},
    packet_helper::PacketReadResult,
};

use crate::{universe::{Universe, UniverseService}, player::{Player, PlayerService}};

// Player connection

pub struct PlayerConnection<U: UniverseService> {
    _phantom: PhantomData<U>,
    is_closing: bool,
    
    player_ptr: *mut (),
    player_process_packet: Option<fn(*mut ()) -> anyhow::Result<u32>>,
    player_process_disconnect: Option<fn(*mut ())>,
}
impl<U: UniverseService> ConnectionService for PlayerConnection<U> {
    const BUFFER_SIZE: u32 = 4_194_304;
    type NetworkManagerServiceType = Universe<U>;

    fn on_receive(
        &mut self,
        connection: &mut Connection<Self::NetworkManagerServiceType>,
    ) -> anyhow::Result<u32> {
        debug_assert!(!self.is_closing, "connection should be closed, but I still got some data!");
        
        if let Some(handle_packet) = self.player_process_packet {
            handle_packet(self.player_ptr)
        } else {
            Ok(connection.read_bytes().len() as u32)
        }
    }

    fn close(mut self) {
        if !self.is_closing {
            self.is_closing = true;

            if let Some(handle_disconnect) = self.player_process_disconnect {
                handle_disconnect(self.player_ptr);
            } else {
                panic!("connection was closed by remote while belonging to a protoplayer, this should never happen");
            }
        }
    }
}

impl<U: UniverseService> PlayerConnection<U> {
    pub fn new() -> Self {
        Self {
            _phantom: PhantomData,
            is_closing: false,
            player_ptr: std::ptr::null_mut(),
            player_process_packet: None,
            player_process_disconnect: None
        }
    }

    pub fn mark_closed(&mut self) {
        self.is_closing = true;
    }

    pub(crate) fn update_player_pointer<T: PlayerService>(&mut self, player: *mut Player<T>) {
        debug_assert!(!self.is_closing);

        self.player_ptr = player as *mut ();

        let process_packet_ptr = Player::<T>::handle_packet  as *const ();
        self.player_process_packet = Some(unsafe { std::mem::transmute(process_packet_ptr) });

        let process_disconnect_ptr = Player::<T>::handle_disconnect  as *const ();
        self.player_process_disconnect = Some(unsafe { std::mem::transmute(process_disconnect_ptr) });

    }
    
    pub(crate) fn clear_player_pointer(&mut self) {
        self.player_process_packet = None;
    }
}
