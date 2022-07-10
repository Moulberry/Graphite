use net::{network_handler::Connection, network_buffer::WriteBuffer};
use crate::{player::{Player, PlayerService}, world::World, universe::{Universe, UniverseService}};

pub struct ProtoPlayer<U: UniverseService> {
    pub write_buffer: WriteBuffer,
    connection: *mut Connection<Universe<U>>,
}

impl <U: UniverseService> ProtoPlayer<U> {
    pub fn new(connection: *mut Connection<Universe<U>>) -> Self {
        Self {
            connection,
            write_buffer: WriteBuffer::new()
        }
    }

    pub fn create_player<P: PlayerService<UniverseServiceType = U>>(mut self, service: P, world: &mut World<P::WorldServiceType>) -> anyhow::Result<Player<P>> {
        // Fill write buffer with required initial packets
        world.write_game_join_packet(&mut self.write_buffer)?;
        world.get_universe().write_brand_packet(&mut self.write_buffer)?;

        let view_position = world.initialize_view_position(&mut self)?;

        // Write the necessary packets to the TCP stream
        unsafe { self.connection.as_mut() }.unwrap().write(self.write_buffer.get_written());

        let player = Player { 
            world,
            service,
            view_position
        };

        Ok(player)
    }
}