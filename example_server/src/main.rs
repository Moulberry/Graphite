use command::brigadier;
use command::types::CommandResult;
use concierge::Concierge;
use concierge::ConciergeService;
use net::network_handler::UninitializedConnection;
use server::player::generic::GenericPlayer;
use server::player::player_vec::PlayerVec;
use server::player::proto_player::ProtoPlayer;
use server::player::PlayerService;
use server::position::Coordinate;
use server::position::Position;
use server::position::Rotation;
use server::universe::Universe;
use server::universe::UniverseService;
use server::world::World;
use server::world::WorldService;

struct MyConciergeImpl;

impl ConciergeService for MyConciergeImpl {
    fn get_serverlist_response(&mut self) -> String {
        "{\
            \"version\": {
                \"name\": \"1.19.1\",
                \"protocol\": 1073741921
            },
            \"players\": {
                \"max\": 0,
                \"online\": 0,
                \"sample\": []
            },
            \"description\": {
                \"text\": \"Hello world\"
            },
            \"favicon\": \"data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAEAAAABACAIAAAAlC+aJAAABGklEQVRo3u2aUQ7EIAhEbcNReiPP6Y16F/djk/1bozJASYffJu08BRxMj957yRxnSR4EIMDbAQTylrvWwdOrNTuAY6+NjhV7YiwDbEg3xVgDUKq3wIgp4rtW1FqYAEwuMAQDk0L/FE/q02TUqVR/tTb4vGkDBaTQjL4xIU/i91gJVNeDV8gZ+HnIorAGCJAAwKIBAACAhixyIvsyKL3Qg0bKqzXnbZlNoXmH/NwitvBkeuC1Ira2lk5daBvDAn6/iH9qAi+Fyva9EDDvlYTxVkJZx/RCBMgHgO1L3IEXAmANn+SV7r0DRk5b0im2BfAfaCRcn/JYkBIXwXejDzmPJZ1iVwCHAfrgD08EIAABCEAAAhCAAAQgwG58AEFWdXlZzlUbAAAAAElFTkSuQmCC\"
        }".into()
    }

    fn accept_player(
        &mut self,
        player_connection: UninitializedConnection,
        _: &concierge::ConciergeConnection<Self>,
    ) {
        #[brigadier("hello", {10..2000}, {})]
        fn my_function(player: &mut dyn GenericPlayer, number: u16, numer2: u8) -> CommandResult {
            println!("number: {}", number);
            println!("numer2: {}", numer2);
            player.send_message(&"Hello from my_function".into());
            Ok(())
        }

        let (dispatcher, packet) = command::minecraft::create_dispatcher_and_brigadier_packet(my_function);

        let universe = server::universe::create_and_start(|| MyUniverseService {
            the_world: World::new(MyWorldService {
                players: PlayerVec::new(),
            }),
        }, dispatcher, packet);
        universe.send(player_connection).unwrap();
    }
}

fn main() {
    //println!("{:?}", packet);
    //dispatcher.dispatch("hello 800 10");

    // server::command::dispatcher::dispatch("hello 100 whatever_we_want 7174");
    Concierge::bind("127.0.0.1:25565", MyConciergeImpl).unwrap();
}

// universe

struct MyUniverseService {
    the_world: World<MyWorldService>,
}

impl UniverseService for MyUniverseService {
    fn handle_player_join(universe: &mut Universe<Self>, proto_player: ProtoPlayer<Self>) {
        universe.service.the_world.handle_player_join(proto_player);
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
}
impl WorldService for MyWorldService {
    type UniverseServiceType = MyUniverseService;
    const CHUNKS_X: usize = 6;
    const CHUNKS_Z: usize = 6;

    fn handle_player_join(
        world: &mut World<Self>,
        mut proto_player: ProtoPlayer<Self::UniverseServiceType>,
    ) {
        proto_player.hardcore = true;

        // make player from proto_player
        world
            .service
            .players
            .add(
                proto_player,
                MyPlayerService {},
                Position {
                    coord: Coordinate {
                        x: 32.0,
                        y: 500.0,
                        z: 32.0,
                    },
                    rot: Rotation {
                        yaw: 0.0,
                        pitch: 0.0,
                    },
                },
            )
            .unwrap();
    }

    fn initialize(world: &World<Self>) {
        world.service.players.initialize(world);
    }

    fn tick(world: &mut World<Self>) {
        world.service.players.tick();
    }

    fn get_player_count(world: &World<Self>) -> usize {
        world.service.players.len()
    }
}

// player

struct MyPlayerService {

}

impl PlayerService for MyPlayerService {
    type UniverseServiceType = MyUniverseService;
    type WorldServiceType = MyWorldService;
}
