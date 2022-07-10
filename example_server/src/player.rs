use net::network_handler::Connection;
use thiserror::Error;

use crate::{
    player_connection::PlayerConnection,
    universe::{Universe, UniverseService, EntityId},
    world::{ChunkViewPosition, World, WorldService},
};

// user defined player service trait

pub trait PlayerService {
    type UniverseServiceType: UniverseService;
    type WorldServiceType: WorldService<UniverseServiceType = Self::UniverseServiceType>;
}

// graphite player

pub struct Player<P: PlayerService> {
    pub service: P,
    pub world: *mut World<P::WorldServiceType>,
    pub entity_id: EntityId,

    pub view_position: ChunkViewPosition,
    pub connection_service: *mut PlayerConnection<P::UniverseServiceType>,
    pub connection: *mut Connection<Universe<P::UniverseServiceType>>,

    pub connection_closed: bool,
}

// graphite player impl

#[derive(Debug, Error)]
#[error("connection has been closed by the remote host")]
struct ConnectionClosedError;

impl<P: PlayerService> Player<P> {
    pub fn tick(&mut self) -> anyhow::Result<()> {
        // todo: replace this error message with something that references documentation instead
        debug_assert!(!self.connection_closed, "`tick` called on player with closed connection. Make sure to remove the Player from your list if the tick function returns ConnectionClosedError");

        if !unsafe { self.connection_service.as_mut().unwrap() }.check_connection_open() {
            self.connection_closed = true;
            return Err(ConnectionClosedError.into());
        }

        Ok(())
    }
}

impl<P: PlayerService> Drop for Player<P> {
    fn drop(&mut self) {
        if !self.connection_closed {
            let successfully_closed =
                unsafe { self.connection_service.as_mut().unwrap() }.close_if_open();
            if successfully_closed {
                unsafe { self.connection.as_mut().unwrap() }.request_close();
            }
        }
    }
}
