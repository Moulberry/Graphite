use crate::{
    player::{Player, PlayerService},
    player_connection::PlayerConnection,
    universe::{Universe, UniverseService},
    world::World,
};
use net::{network_buffer::WriteBuffer, network_handler::Connection};

pub struct ProtoPlayer<U: UniverseService> {
    pub write_buffer: WriteBuffer,
    connection_service: *mut PlayerConnection<U>,
    connection: *mut Connection<Universe<U>>,
}

impl<U: UniverseService> ProtoPlayer<U> {
    pub fn new(connection: (*mut Connection<Universe<U>>, *mut PlayerConnection<U>)) -> Self {
        Self {
            write_buffer: WriteBuffer::new(),
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
        world.write_game_join_packet(&mut self.write_buffer)?;
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
            view_position,
            connection: self.connection,
            connection_service: self.connection_service,
            deleted: false
        };

        Ok(player)
    }
}
