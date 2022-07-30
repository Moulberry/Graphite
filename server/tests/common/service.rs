use super::FakePlayerConnection;
use protocol::types::GameProfile;
use server::{
    entity::position::{Coordinate, Position, Rotation},
    player::{player_vec::PlayerVec, PlayerService},
    universe::{Universe, UniverseService},
    world::{TickPhase, World, WorldService},
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
        the_world: World::new(DummyWorldService {
            players: PlayerVec::new(),
        }),
    };

    let pinned = Box::pin(Universe::create_dummy(service));
    DummyUniverseService::initialize(&pinned);
    pinned
}

pub struct DummyUniverseService {
    pub the_world: World<DummyWorldService>,
}
impl UniverseService for DummyUniverseService {
    type ConnectionReferenceType = *mut FakePlayerConnection;

    fn handle_player_join(
        universe: &mut Universe<Self>,
        proto_player: server::player::proto_player::ProtoPlayer<Self>,
    ) {
        universe.service.the_world.handle_player_join(proto_player);
    }

    fn initialize(universe: &Universe<Self>) {
        universe.service.the_world.initialize(universe);
    }

    fn tick(universe: &mut Universe<Self>) {
        universe.service.the_world.tick();
    }

    fn get_player_count(universe: &Universe<Self>) -> usize {
        DummyWorldService::get_player_count(&universe.service.the_world)
    }
}

pub struct DummyWorldService {
    pub players: PlayerVec<DummyPlayerService>,
}
impl WorldService for DummyWorldService {
    type UniverseServiceType = DummyUniverseService;

    const CHUNKS_X: usize = 5;
    const CHUNKS_Z: usize = 5;
    const CHUNK_VIEW_DISTANCE: u8 = 8;
    const ENTITY_VIEW_DISTANCE: u8 = 1;

    fn handle_player_join(
        world: &mut World<Self>,
        proto_player: server::player::proto_player::ProtoPlayer<Self::UniverseServiceType>,
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

    fn initialize(world: &World<Self>) {
        world.service.players.initialize(world);
    }

    unsafe fn tick(world: &mut World<Self>, phase: TickPhase) {
        world.service.players.tick(phase);
    }

    fn get_player_count(world: &World<Self>) -> usize {
        world.service.players.len()
    }
}

pub struct DummyPlayerService;
impl PlayerService for DummyPlayerService {
    const FAST_PACKET_RESPONSE: bool = true;
    type UniverseServiceType = DummyUniverseService;
    type WorldServiceType = DummyWorldService;
}
