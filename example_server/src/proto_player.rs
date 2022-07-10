use crate::{
    player::{Player, PlayerService},
    player_connection::PlayerConnection,
    universe::{EntityId, Universe, UniverseService},
    world::World,
};
use net::{network_buffer::WriteBuffer, network_handler::Connection};

pub struct ProtoPlayer<U: UniverseService> {
    pub hardcore: bool,

    pub(crate) write_buffer: WriteBuffer,
    pub(crate) entity_id: EntityId,
    connection_service: *mut PlayerConnection<U>,
    connection: *mut Connection<Universe<U>>,
}

impl<U: UniverseService> ProtoPlayer<U> {
    pub fn new(
        connection: (*mut Connection<Universe<U>>, *mut PlayerConnection<U>),
        entity_id: EntityId,
    ) -> Self {
        Self {
            hardcore: false,

            write_buffer: WriteBuffer::new(),
            entity_id,

            connection: connection.0,
            connection_service: connection.1,
        }
    }

    pub fn create_player<P: PlayerService<UniverseServiceType = U>>(
        mut self,
        service: P,
        world: &mut World<P::WorldServiceType>,
    ) -> anyhow::Result<Player<P>> {
        // Fill write buffer with required initial packets
        world.write_game_join_packet(&mut self)?;
        world
            .get_universe()
            .write_brand_packet(&mut self.write_buffer)?;

        let view_position = world.initialize_view_position(&mut self)?;

        // Write the necessary packets to the TCP stream
        unsafe { self.connection.as_mut() }
            .unwrap()
            .write(self.write_buffer.get_written());

        let player = Player {
            world,
            service,
            entity_id: self.entity_id,
            view_position,
            connection: self.connection,
            connection_service: self.connection_service,
            connection_closed: false,
        };

        Ok(player)
    }
}
