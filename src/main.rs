use std::{sync::{Arc, Mutex}, thread};

use glam::DVec3;
use graphite_anvil::ChunkCoord;
use graphite_concierge::LoginInformation;
use graphite_core_server::{world::WorldExtension, ConfiguringPlayer, player::{PlayerExtension, Player}, Universe, WorldId, UniverseExtension, entity::{EntityExtension, Entity, entity_view_controller::DebugEntityViewController}, types::AABB, Behaviour, inventory::item_stack::ItemStack};
use mio::net::TcpStream;

struct MyEntity {

}

impl EntityExtension for MyEntity {
    type World = MyWorld;
    type View = DebugEntityViewController;

    fn tick(entity: &mut Entity<Self>) {
        entity.position.y -= 0.1;
    }

    fn create_view_controller(&mut self) -> Self::View {
        DebugEntityViewController::new()
    }
}

struct MyPlayer {

}

impl PlayerExtension for MyPlayer {
    type World = MyWorld;

    fn swap_item_with_off_hand(player: &mut Player<Self>) -> Behaviour {
        let mut delta = player.view_vector().as_dvec3() * 2.5;
        if delta.y > 1.5 {
            delta.y = 1.5;
        }

        let aabb = AABB::new(player.position() - DVec3::new(0.3, 0.0, 0.3), 
            player.position() + DVec3::new(0.3, 1.8, 0.3)).unwrap();
        let moved = player.world_mut().move_bounding_box_with_collision(aabb, delta);

        player.teleport_position(player.position() + moved);

        let mut velocity = player.view_vector() * 0.7875;
        if velocity.y > 0.0 {
            velocity.y = 0.0;
        } 
        player.set_velocity(velocity);

        player.flush_packets();

        Behaviour::Pass
    }

    fn set_creative_mode_slot(_player: &mut Player<Self>, _slot: i16, _item_stack: Option<&ItemStack>) -> Behaviour {
        Behaviour::Vanilla
    }
}

struct MyWorld {

}

impl WorldExtension for MyWorld {
    type Universe = MyUniverse;
    const CHUNKS_X: i32 = 43;
    const CHUNKS_Z: i32 = 43;
    const VIEW_DISTANCE: u8 = 16;
}

struct MyUniverse {
    world: WorldId
}

impl UniverseExtension for MyUniverse {
    fn init(universe: &mut Universe<Self>) -> Self {
        let anvil_world = graphite_anvil::load_anvil(ChunkCoord::new(-10, -10), 24,
            ChunkCoord::new(32, 32), include_dir::include_dir!("src/world/")).unwrap();

        let world_id = universe.create_world(MyWorld {}, anvil_world.into());

        universe.world(world_id).unwrap().spawn_new_entity(DVec3::new(88.0, 200.0, 88.0), MyEntity {});

        MyUniverse {
            world: world_id
        }
    }

    fn spawn_player(universe: &mut Universe<Self>, player: ConfiguringPlayer<Self>) {
        let world_id = universe.extension().world;
        universe.world::<MyWorld>(world_id).unwrap().spawn_new_player(DVec3::new(24.0, 200.0, 24.0),
            player, MyPlayer {});
    }
}

fn main() {
    println!("Starting concierge on port 25565!");

    let (mut core_network, mut core_sender) = graphite_core_server::Universe::<MyUniverse>::new();

    let sender = Box::new(move |login_information: LoginInformation, stream: TcpStream| {
        core_sender.send(stream, login_information);
    });

    thread::spawn(move || {
        graphite_concierge::listen("0.0.0.0:25565", sender, Arc::new(Mutex::new(
            r#"{
                "version": {
                    "name": "1.20.4",
                    "protocol": 765
                },
                "players": {
                    "max": 100,
                    "online": 5,
                    "sample": [
                        {
                            "name": "thinkofdeath",
                            "id": "4566e69f-c907-48ee-8d71-d7ba5aa00d20"
                        }
                    ]
                },
                "description": {
                    "text": "Hello world"
                },
                "favicon": "data:image/png;base64,<data>",
                "enforcesSecureChat": true,
                "previewsChat": true
            }"#.into()
        )));
    });

    println!("Starting core server");

    core_network.listen().unwrap();
}
