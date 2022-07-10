use net::network_handler::Connection;

use crate::{
    player_connection::PlayerConnection,
    universe::{Universe, UniverseService},
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

    pub view_position: ChunkViewPosition,
    pub connection_service: *mut PlayerConnection<P::UniverseServiceType>,
    pub connection: *mut Connection<Universe<P::UniverseServiceType>>,

    pub deleted: bool
}

// graphite player impl

impl<P: PlayerService> Player<P> {
    pub fn tick(&mut self) -> bool {
        // todo: replace this error message with something that references documentation instead
        debug_assert!(!self.deleted, "`tick` called on player that was deleted. Make sure to remove the Player from your list if the tick function returns false");

        if !unsafe { self.connection_service.as_mut().unwrap() }.check_connection_open() {
            self.deleted = true;
            return false;
        }

        println!("player tick!");
        true
    }
}
