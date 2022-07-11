use concierge::Concierge;
use concierge::ConciergeService;
use net::network_handler::UninitializedConnection;
use player::PlayerService;
use player_vec::PlayerVec;
use proto_player::ProtoPlayer;
use universe::Universe;
use universe::UniverseService;
use world::World;
use world::WorldService;

mod error;
mod player;
mod player_connection;
mod player_vec;
mod proto_player;
mod universe;
mod world;

struct MyConciergeImpl {
    counter: u8,
}

impl ConciergeService for MyConciergeImpl {
    fn get_serverlist_response(&mut self) -> String {
        self.counter += 1;
        format!("{{\
            \"version\": {{
                \"name\": \"1.19.1\",
                \"protocol\": 1073741921
            }},
            \"players\": {{
                \"max\": 100,
                \"online\": {},
                \"sample\": []
            }},
            \"description\": {{
                \"text\": \"Hello world\"
            }},
            \"favicon\": \"data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAEAAAABACAIAAAAlC+aJAAABGklEQVRo3u2aUQ7EIAhEbcNReiPP6Y16F/djk/1bozJASYffJu08BRxMj957yRxnSR4EIMDbAQTylrvWwdOrNTuAY6+NjhV7YiwDbEg3xVgDUKq3wIgp4rtW1FqYAEwuMAQDk0L/FE/q02TUqVR/tTb4vGkDBaTQjL4xIU/i91gJVNeDV8gZ+HnIorAGCJAAwKIBAACAhixyIvsyKL3Qg0bKqzXnbZlNoXmH/NwitvBkeuC1Ira2lk5daBvDAn6/iH9qAi+Fyva9EDDvlYTxVkJZx/RCBMgHgO1L3IEXAmANn+SV7r0DRk5b0im2BfAfaCRcn/JYkBIXwXejDzmPJZ1iVwCHAfrgD08EIAABCEAAAhCAAAQgwG58AEFWdXlZzlUbAAAAAElFTkSuQmCC\"
        }}", self.counter)
    }

    fn accept_player(
        &mut self,
        player_connection: UninitializedConnection,
        protoplayer: &concierge::ConciergeConnection<Self>,
    ) {
        println!("managed to get connection: {:?}", protoplayer.username);
        let universe = universe::create_and_start(|| MyUniverseService {
            the_world: World::new(MyWorldService {
                players: player_vec::PlayerVec::new(),
                counter: 0,
            }),
        });
        universe.send(player_connection).unwrap();
    }
}

fn main() {
    /*let mut indices = vec!();
    for i in 0..1000 {
        let index = rand::thread_rng().next_u64() as usize % (1000 - i);
        indices.push(index);
    }
    println!("{:?}", indices);*/

    Concierge::bind("127.0.0.1:25565", MyConciergeImpl { counter: 0 }).unwrap();
}

// universe

struct MyUniverseService {
    the_world: World<MyWorldService>,
}

impl UniverseService for MyUniverseService {
    fn handle_player_join(universe: &mut Universe<Self>, proto_player: ProtoPlayer<Self>) {
        universe.service.the_world.send_player_to(proto_player);
    }

    fn initialize(universe: &Universe<Self>) {
        universe.service.the_world.initialize(universe);
    }

    fn tick(universe: &mut Universe<Self>) {
        universe.service.the_world.tick();
    }

    fn get_player_count(universe: &Universe<Self>) -> usize {
        MyWorldService::get_player_count(&universe.service.the_world)
    }
}

// world

// DungeonPlayer
// SpectatingPlayer

struct MyWorldService {
    players: PlayerVec<MyPlayerService>,
    counter: usize,
}
impl WorldService for MyWorldService {
    type UniverseServiceType = MyUniverseService;

    fn handle_player_join(
        world: &mut World<Self>,
        mut proto_player: ProtoPlayer<Self::UniverseServiceType>,
    ) {
        proto_player.hardcore = true;

        // make player from proto_player
        world
            .service
            .players
            .add(proto_player, MyPlayerService {})
            .unwrap();
    }

    fn initialize(world: &World<Self>) {
        world.service.players.initialize(world);
    }

    fn tick(world: &mut World<Self>) {
        if world.service.players.len() > 0 {
            world.service.counter += 1;
            if world.service.counter > 100 {
                world.service.counter = 0;
                world.service.players.remove(0);
            }
        }
        world.service.players.tick();
    }

    fn get_player_count(world: &World<Self>) -> usize {
        world.service.players.len()
    }
}

// player

struct MyPlayerService {}

impl PlayerService for MyPlayerService {
    type UniverseServiceType = MyUniverseService;
    type WorldServiceType = MyWorldService;
}
