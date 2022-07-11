use concierge::ConciergeConnection;
use net::{network_handler::Connection, network_buffer::WriteBuffer, packet_helper::PacketReadResult};
use sticky::Unsticky;
use thiserror::Error;

use crate::{
    player_connection::{PlayerConnection},
    universe::{EntityId, Universe, UniverseService},
    world::{ChunkViewPosition, World, WorldService}, proto_player::{ProtoPlayer, ConnectionReference},
};

// user defined player service trait

pub trait PlayerService
where Self: Sized {
    type UniverseServiceType: UniverseService;
    type WorldServiceType: WorldService<UniverseServiceType = Self::UniverseServiceType>;
}

// graphite player

pub struct Player<P: PlayerService> {
    pub service: P,

    view_position: ChunkViewPosition,
    entity_id: EntityId,

    world: *mut World<P::WorldServiceType>,

    connection: ConnectionReference<P::UniverseServiceType>
}

// graphite player impl

#[derive(Debug, Error)]
#[error("connection has been closed by the remote host")]
struct ConnectionClosedError;

impl<P: PlayerService> Player<P> {
    pub(crate) fn new(service: P, world: &mut World<P::WorldServiceType>, entity_id: EntityId,
                    view_position: ChunkViewPosition, connection: ConnectionReference<P::UniverseServiceType>) -> Self {
        Self {
            service,
            world,
            entity_id,
            view_position,
            connection
        }
    }

    pub fn tick(&mut self) -> anyhow::Result<()> {
        Ok(())
    }

    pub fn handle_packet(player: *mut Player<P>) -> anyhow::Result<u32> {
        let player = unsafe { player.as_mut() }.unwrap();
        
        // println!("got process!");
        // println!("i have entity id: {:?}", player.entity_id);

        Ok(0)
    }

    pub fn handle_disconnect(player: *mut Player<P>) {
        unsafe {
            let player: &mut Player<P> = player.as_mut().unwrap();
            player.connection.forget();
        }

        println!("closed by remote");

        todo!();
        // todo: remove self from PlayerList containing this

        // todo: notify world/universe of disconnect
    }
}

unsafe impl<P: PlayerService> Unsticky for Player<P> {
    type UnstuckType = (ProtoPlayer<P::UniverseServiceType>, P);

    fn update_pointer(&mut self, _: usize) {
        // todo: use index to be able to remove self from sticky
        let ptr: *mut Player<P> = self;
        self.connection.update_player_pointer(ptr);
    }

    fn unstick(self) -> Self::UnstuckType {
        (
            ProtoPlayer::new(self.connection, self.entity_id),
            self.service
        )
    }
}