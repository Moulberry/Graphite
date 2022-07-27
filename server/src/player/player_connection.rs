use std::marker::PhantomData;

use net::network_handler::{Connection, ConnectionService, ConnectionSlab};

use crate::universe::{Universe, UniverseService};

use super::player::{Player, PlayerService};

// Connection reference
#[derive(Debug)]
pub struct ConnectionReference<U: UniverseService> {
    closed: bool,
    connection_index: u16,
    connection_slab: *mut ConnectionSlab<Universe<U>>,
}

impl<U: UniverseService> ConnectionReference<U> {
    pub(crate) fn update_player_pointer<P: PlayerService>(&mut self, player: *mut Player<P>) {
        self.get_connection_mut().1.update_player_pointer(player);
    }

    fn get_connection(&self) -> &(Connection<Universe<U>>, PlayerConnection<U>) {
        debug_assert!(!self.closed);
        unsafe {
            (&mut *self.connection_slab)
                .get(self.connection_index as _)
                .expect("connection should have notified us of it being invalid")
        }
    }

    fn get_connection_mut(&mut self) -> &mut (Connection<Universe<U>>, PlayerConnection<U>) {
        debug_assert!(!self.closed);
        unsafe {
            (&mut *self.connection_slab)
                .get_mut(self.connection_index as _)
                .expect("connection should have notified us of it being invalid")
        }
    }

    pub(crate) fn new(
        connection_slab: &mut ConnectionSlab<Universe<U>>,
        connection_index: u16,
    ) -> Self {
        Self {
            closed: false,
            connection_slab,
            connection_index,
        }
    }

    /// # Safety
    /// This method should only be called if it is known that
    /// the connection pointed to has been closed as well
    ///
    /// If this is not the case, calling this method may result in
    /// the connection living forever
    pub(crate) unsafe fn forget(&mut self) {
        self.closed = true;
    }

    pub(crate) fn read_bytes(&self) -> &[u8] {
        self.get_connection().0.read_bytes()
    }

    pub(crate) fn write_bytes(&mut self, bytes: &[u8]) {
        self.get_connection_mut().0.write(bytes);
    }
}

// Causes the connection to close when the ConnectionReference is dropped
impl<U: UniverseService> Drop for ConnectionReference<U> {
    fn drop(&mut self) {
        if !self.closed {
            let (connection, player_connection) = self.get_connection_mut();
            player_connection.teardown();
            connection.request_close();
        }
    }
}

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
        _: &mut Connection<Self::NetworkManagerServiceType>,
    ) -> anyhow::Result<u32> {
        debug_assert!(
            !self.is_closing,
            "connection should be closed, but I still got some data!"
        );

        if let Some(handle_packet) = self.player_process_packet {
            // Call handle_packet on player, via fn ptr
            handle_packet(self.player_ptr)
        } else {
            panic!("data was read from connection, but player wasn't created");
        }
    }

    fn close(mut self) {
        debug_assert!(
            !self.is_closing,
            "tried to close connection twice"
        );

        self.is_closing = true;

        if let Some(handle_disconnect) = self.player_process_disconnect {
            // Call handle_disconnect on player, via fn ptr
            handle_disconnect(self.player_ptr);
        } else {
            panic!("connection was closed by remote, but player wasn't created");
        }
    }
}

impl<U: UniverseService> PlayerConnection<U> {
    pub(crate) fn new() -> Self {
        Self {
            _phantom: PhantomData,
            is_closing: false,
            player_ptr: std::ptr::null_mut(),
            player_process_packet: None,
            player_process_disconnect: None,
        }
    }

    pub(crate) fn teardown(&mut self) {
        self.is_closing = true;
        self.player_process_packet = None;
        self.player_process_disconnect = None;
    }

    pub(crate) fn update_player_pointer<T: PlayerService>(&mut self, player: *mut Player<T>) {
        debug_assert!(
            !self.is_closing,
            "tried to update connection, but the connection was already closed"
        );

        // Set ptr to player object
        self.player_ptr = player as *mut ();

        // Set ptr to process_packet function
        let process_packet_ptr = Player::<T>::handle_packets as *const ();
        self.player_process_packet = Some(unsafe { std::mem::transmute(process_packet_ptr) });

        // Set ptr to process_disconnection function
        let process_disconnect_ptr = Player::<T>::handle_disconnect as *const ();
        self.player_process_disconnect =
            Some(unsafe { std::mem::transmute(process_disconnect_ptr) });
    }
}
