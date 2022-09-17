use super::FakePlayerConnection;
use graphite_mc_protocol::types::GameProfile;
use graphite_server::{
    entity::position::{Coordinate, Position, Rotation},
    inventory::inventory_handler::VanillaPlayerInventory,
    player::{player_vec::PlayerVec, PlayerService},
    universe::{Universe, UniverseService},
    world::{TickPhase, World, WorldService}, UniverseTicker,
};
use std::pin::Pin;

pub fn create_game_profile() -> GameProfile {
    GameProfile {
        username: "Moulberry".into(),
        uuid: 0xd0e05de76067454dbeaec6d19d886191,
        properties: vec![],
    }
}

pub fn create_universe_and_player() -> (
    Pin<Box<Universe<DummyUniverseService>>>,
    Pin<Box<FakePlayerConnection>>,
) {
    let mut universe = create_universe();
    let player = create_player(&mut universe);
    (universe, player)
}

pub fn create_player(
    universe: &mut Universe<DummyUniverseService>,
) -> Pin<Box<FakePlayerConnection>> {
    let mut conn = Box::from(FakePlayerConnection::new());
    universe.handle_player_connect(conn.as_mut(), create_game_profile());
    Pin::from(conn)
}

pub fn create_universe() -> Pin<Box<Universe<DummyUniverseService>>> {
    let service = DummyUniverseService {
        the_world: World::new_with_default_chunks(DummyWorldService {
            players: PlayerVec::new(),
        }, 5, 24, 5),
    };

    let pinned = Box::pin(Universe::create_dummy(service));
    DummyUniverseService::initialize(&pinned);
    pinned
}

#[derive(UniverseTicker)]
pub struct DummyUniverseService {
    pub the_world: World<DummyWorldService>,
}

impl UniverseService for DummyUniverseService {
    type ConnectionReferenceType = *mut FakePlayerConnection;

    fn handle_player_join(
        universe: &mut Universe<Self>,
        proto_player: graphite_server::player::proto_player::ProtoPlayer<Self>,
    ) {
        universe.service.the_world.handle_player_join(proto_player);
    }
}

pub struct DummyWorldService {
    pub players: PlayerVec<DummyPlayerService>,
}
impl WorldService for DummyWorldService {
    type UniverseServiceType = DummyUniverseService;

    const CHUNK_VIEW_DISTANCE: u8 = 8;
    const ENTITY_VIEW_DISTANCE: u8 = 1;

    fn handle_player_join(
        world: &mut World<Self>,
        proto_player: graphite_server::player::proto_player::ProtoPlayer<Self::UniverseServiceType>,
    ) {
        world
            .service
            .players
            .add(
                proto_player,
                DummyPlayerService,
                Position {
                    coord: Coordinate {
                        x: 40.0,
                        y: 224.0,
                        z: 40.0,
                    },
                    rot: Rotation::default(),
                },
            )
            .unwrap();
    }
}

pub struct DummyPlayerService;
impl PlayerService for DummyPlayerService {
    const FAST_PACKET_RESPONSE: bool = true;
    type UniverseServiceType = DummyUniverseService;
    type WorldServiceType = DummyWorldService;
    type InventoryHandlerType = VanillaPlayerInventory;
}
