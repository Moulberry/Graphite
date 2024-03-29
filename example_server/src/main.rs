use std::env;
use std::fs::File;
use std::io::Read;
use std::io::Write;
use std::sync::mpsc::Sender;
use std::time::Instant;

use graphite_command::brigadier;
use graphite_command::types::CommandResult;
use graphite_concierge::Concierge;
use graphite_concierge::ConciergeService;
use graphite_net::network_handler::UninitializedConnection;
use graphite_mc_protocol::types::GameProfile;
use graphite_mc_protocol::types::Pose;
use graphite_server::UniverseTicker;
use graphite_server::WorldTicker;
use rand::Rng;
use graphite_server::entity::components::BasicEntity;
use graphite_server::entity::components::PlayerNPC;
use graphite_server::entity::components::Spinalla;
use graphite_server::entity::position::Coordinate;
use graphite_server::entity::position::Position;
use graphite_server::entity::position::Rotation;
use graphite_server::gamemode::GameMode;
use graphite_server::inventory::inventory_handler::VanillaPlayerInventory;
use graphite_server::player::player_connection::ConnectionReference;
use graphite_server::player::player_vec::PlayerVec;
use graphite_server::player::proto_player::ProtoPlayer;
use graphite_server::player::Player;
use graphite_server::player::PlayerService;
use graphite_server::universe::Universe;
use graphite_server::universe::UniverseService;
use graphite_server::world::TickPhase;
use graphite_server::world::World;
use graphite_server::world::WorldService;

struct MyConciergeImpl {
    universe_sender: Sender<(UninitializedConnection, GameProfile)>,
}

impl ConciergeService for MyConciergeImpl {
    fn get_serverlist_response(&mut self) -> String {
        "{\
            \"version\": {
                \"name\": \"1.19.1\",
                \"protocol\": 760
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
        mut concierge_connection: graphite_concierge::ConciergeConnection<Self>,
    ) {
        let join_data = (
            player_connection,
            concierge_connection.game_profile.take().unwrap(),
        );

        self.universe_sender.send(join_data).unwrap();
    }
}

fn main() {
    #[brigadier("hello", {10..2000}, {})]
    fn my_function(player: &mut Player<MyPlayerService>, number: u16, number2: u8) -> CommandResult {
        println!("number: {}", number);
        println!("number2: {}", number2);
        player.send_message("Hello from my_function");
        Ok(())
    }

    #[brigadier("entity_test", {})]
    fn entity_test(player: &mut Player<MyPlayerService>, entity_type: u8) -> CommandResult {
        player.send_message("Hello from MyPlayerService");

        for _ in 0..1 {
            let entity_id = player.get_world_mut().get_universe().new_entity_id();

            let test_entity = BasicEntity {
                entity_id,
                entity_type: entity_type as _,
            };

            let entity = (Spinalla {
                direction: (
                    rand::thread_rng().gen_range(-1.0..1.0),
                    rand::thread_rng().gen_range(-1.0..1.0),
                ),
                rotation: Rotation {
                    yaw: 0.0,
                    pitch: 0.0,
                },
            },);

            player.get_world_mut().push_entity(
                (),
                Coordinate {
                    x: player.position.coord.x,
                    y: player.position.coord.y,
                    z: player.position.coord.z,
                },
                test_entity,
                entity_id,
            );
        }

        Ok(())
    }

    #[brigadier("spawn_player")]
    fn spawn_player(player: &mut Player<MyPlayerService>) -> CommandResult {
        let entity_id = player.get_world_mut().get_universe().new_entity_id();

        let player_npc = PlayerNPC {
            entity_id,
            uuid: rand::thread_rng().gen(),
            username: "Friend".into(),
        };

        let entity = (Spinalla {
            direction: (
                rand::thread_rng().gen_range(-1.0..1.0),
                rand::thread_rng().gen_range(-1.0..1.0),
            ),
            rotation: Rotation {
                yaw: 0.0,
                pitch: 0.0,
            },
        },);

        player.get_world_mut().push_entity(
            entity,
            Coordinate {
                x: player.position.coord.x,
                y: player.position.coord.y,
                z: player.position.coord.z,
            },
            player_npc,
            entity_id,
        );
        Ok(())
    }

    #[brigadier("fly")]
    fn fly(player: &mut Player<MyPlayerService>) -> CommandResult {
        player.abilities.set_flying(!player.abilities.is_flying);
        Ok(())
    }

    #[brigadier("glow_up", {})]
    fn glow_up(player: &mut Player<MyPlayerService>, flags: u8) -> CommandResult {
        player.metadata.set_shared_flags(flags);
        player.metadata.set_pose(Pose::FallFlying);

        Ok(())
    }

    #[brigadier("save")]
    fn save(player: &mut Player<MyPlayerService>) -> CommandResult {
        let world = player.get_world();
        let chunks = world.get_chunks();
        let output = graphite_magma::to_magma(chunks, 0);

        let dest_path = env::current_dir().unwrap().join("world.magma");
        let mut f = File::create(&dest_path).unwrap();
        f.write_all(output.unwrap().as_slice()).unwrap();

        Ok(())
    }

    #[brigadier("gamemode", {})]
    fn gamemode(player: &mut Player<MyPlayerService>, id: u8) -> CommandResult {
        let gamemode = match id {
            0 => GameMode::Survival,
            1 => GameMode::Creative,
            2 => GameMode::Adventure,
            3 => GameMode::Spectator,
            _ => panic!("unknown gamemode"),
        };
        player.abilities.gamemode = gamemode;

        Ok(())
    }

    my_function.merge(entity_test).unwrap();
    my_function.merge(spawn_player).unwrap();
    my_function.merge(fly).unwrap();
    my_function.merge(glow_up).unwrap();
    my_function.merge(gamemode).unwrap();
    my_function.merge(save).unwrap();

    let (dispatcher, packet) =
        graphite_command::minecraft::create_dispatcher_and_brigadier_packet(my_function);

    let universe_sender = Universe::create_and_start(
        || MyUniverseService {
            the_world: {
                let dest_path = env::current_dir().unwrap().join("world.magma");
                let mut f = File::open(&dest_path).unwrap();

                let mut vec = Vec::new();
                f.read_to_end(&mut vec).unwrap();


                let start = Instant::now();
                let (chunk_list, _) = graphite_magma::from_magma(vec.as_slice()).unwrap();
                println!("loading world took: {:?}", Instant::now().duration_since(start));

                World::new(MyWorldService {
                    players: PlayerVec::new(),
                }, chunk_list)
                // World::new_with_default_chunks(MyWorldService {
                //     players: PlayerVec::new(),
                // }, 6, 24, 6)
            },
        },
        Some((dispatcher, packet)),
    );

    Concierge::bind("127.0.0.1:25565", MyConciergeImpl { universe_sender }).unwrap();
}

// universe

#[derive(UniverseTicker)]
struct MyUniverseService {
    the_world: World<MyWorldService>,
}

impl UniverseService for MyUniverseService {
    type ConnectionReferenceType = ConnectionReference<Self>;

    fn handle_player_join(universe: &mut Universe<Self>, proto_player: ProtoPlayer<Self>) {
        universe.service.the_world.handle_player_join(proto_player);
    }
}

// world

#[derive(WorldTicker)]
struct MyWorldService {
    players: PlayerVec<MyPlayerService>,
}

impl WorldService for MyWorldService {
    type UniverseServiceType = MyUniverseService;
    type ParentWorldServiceType = Self;
    const CHUNK_VIEW_DISTANCE: u8 = 8;
    const ENTITY_VIEW_DISTANCE: u8 = 1;

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
                        y: 224.0,
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
}

// player

struct MyPlayerService {}

impl PlayerService for MyPlayerService {
    const FAST_PACKET_RESPONSE: bool = true;

    type UniverseServiceType = MyUniverseService;
    type WorldServiceType = MyWorldService;
    type InventoryHandlerType = VanillaPlayerInventory;
}
