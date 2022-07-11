use crate::{
    player::{Player, PlayerService},
    player_connection::PlayerConnection,
    universe::{EntityId, Universe, UniverseService},
    world::World, player_vec::PlayerVec,
};
use net::{network_buffer::WriteBuffer, network_handler::{Connection, ConnectionSlab}};

// Connection reference
pub struct ConnectionReference<U: UniverseService> {
    closed: bool,
    connection_slab: *mut ConnectionSlab<Universe<U>>,
    connection_index: u16
}

impl <U: UniverseService> ConnectionReference<U> {
    pub(crate) fn update_player_pointer<P: PlayerService>(&self, player: *mut Player<P>) {
        self.get_connection().1.update_player_pointer(player);
    }

    fn get_connection(&self) -> &mut (Connection<Universe<U>>, PlayerConnection<U>) {
        debug_assert!(!self.closed);

        unsafe {
            let connection_slab: &mut ConnectionSlab<Universe<U>> = self.connection_slab.as_mut().unwrap();
            connection_slab.get_mut(self.connection_index as _)
                    .expect("connection should have notified us of it being invalid")
        }
    }

    pub(crate) fn new(connection_slab: &mut ConnectionSlab<Universe<U>>, connection_index: u16) -> Self {
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

    pub(crate) fn write(&mut self, bytes: &[u8]) {
        self.get_connection().0.write(bytes);
    }
}

impl <U: UniverseService> Drop for ConnectionReference<U> {
    fn drop(&mut self) {
        println!("dropping! {}", self.closed);
        if !self.closed {
            let (connection, player_connection) = self.get_connection();

            player_connection.clear_player_pointer();
            player_connection.mark_closed();
            connection.request_close();
        }
    }
}

// Proto player

pub struct ProtoPlayer<U: UniverseService> {
    pub hardcore: bool,

    pub(crate) write_buffer: WriteBuffer,
    pub(crate) entity_id: EntityId,
    // username
    // uuid

    connection: ConnectionReference<U>,
}

impl<U: UniverseService> ProtoPlayer<U> {
    pub fn new(
        connection: ConnectionReference<U>,
        entity_id: EntityId,
    ) -> Self {
        Self {
            hardcore: false,

            write_buffer: WriteBuffer::new(),
            entity_id,

            connection,
        }
    }

    pub(crate) fn create_player<P: PlayerService<UniverseServiceType = U>>(
        mut self,
        service: P,
        world: &mut World<P::WorldServiceType>
    ) -> anyhow::Result<Player<P>> {
        // Fill write buffer with required initial packets
        
        // todo: dont send all these packets if the player is in the same world
        // i.e. the player had it's PlayerService changed

        world.write_game_join_packet(&mut self)?;
        world
            .get_universe()
            .write_brand_packet(&mut self.write_buffer)?;

        let view_position = world.initialize_view_position(&mut self)?;

        // Write the necessary packets to the TCP stream
        self.connection.write(self.write_buffer.get_written());

        let player = Player::new(
            service,
            world,
            self.entity_id,
            view_position,
            self.connection
        );

        Ok(player)
    }
}
