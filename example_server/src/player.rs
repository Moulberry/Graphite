use crate::{world::{WorldService, World, ChunkViewPosition}, universe::UniverseService};

// user defined player service trait

pub trait PlayerService {
    type UniverseServiceType: UniverseService;
    type WorldServiceType: WorldService<UniverseServiceType = Self::UniverseServiceType>;
}

// graphite player

pub struct Player<P: PlayerService> {
    pub service: P,
    pub world: *mut World<P::WorldServiceType>,

    pub view_position: ChunkViewPosition
}

// graphite player impl

// ...
