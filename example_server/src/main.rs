use concierge::Concierge;
use concierge::ConciergeService;
use net::network_handler::UninitializedConnection;
use player::Player;
use player::PlayerService;
use proto_player::ProtoPlayer;
use universe::Universe;
use universe::UniverseService;
use world::World;
use world::WorldService;

mod player;
mod player_connection;
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
        protoplayer: &concierge::ProtoPlayer<Self>,
    ) {
        println!("managed to get connection: {:?}", protoplayer.username);
        let universe = universe::create_and_start(|| MyUniverseService { the_world: None });
        universe.send(player_connection).unwrap();
    }
}

fn main() {
    //typemap::my_func();
    Concierge::bind("127.0.0.1:25565", MyConciergeImpl { counter: 0 }).unwrap();
}

// universe

struct MyUniverseService {
    the_world: Option<World<MyWorldService>>,
}

impl UniverseService for MyUniverseService {
    fn handle_player_join(universe: &mut Universe<Self>, proto_player: ProtoPlayer<Self>) {
        universe
            .service
            .the_world
            .as_mut()
            .unwrap()
            .send_player_to(proto_player);
    }

    fn initialize(universe: &mut Universe<Self>) {
        let world = World::new(
            MyWorldService {
                players: Vec::new(),
            },
            universe,
        );
        universe.service.the_world = Some(world);
    }

    fn tick(universe: &mut Universe<Self>) {
        universe.service.the_world.as_mut().unwrap().tick();
    }

    fn get_player_count(universe: &mut Universe<Self>) -> usize {
        MyWorldService::get_player_count(universe.service.the_world.as_mut().unwrap())
    }
}

// world

struct MyWorldService {
    players: Vec<Player<MyPlayerService>>,
}
impl WorldService for MyWorldService {
    type UniverseServiceType = MyUniverseService;

    fn handle_player_join(
        world: &mut World<Self>,
        mut proto_player: ProtoPlayer<Self::UniverseServiceType>,
    ) {
        proto_player.hardcore = true;

        // make player from proto_player
        let player = proto_player.create_player(MyPlayerService {}, world);

        // push
        world.service.players.push(player.unwrap());
    }

    fn tick(world: &mut World<Self>) {
        world.service.players.retain_mut(|p| p.tick().is_ok());
    }

    fn get_player_count(world: &mut World<Self>) -> usize {
        world.service.players.len()
    }
}

// player

struct MyPlayerService {}

impl PlayerService for MyPlayerService {
    type UniverseServiceType = MyUniverseService;
    type WorldServiceType = MyWorldService;
}
