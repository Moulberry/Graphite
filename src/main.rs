use std::{sync::{atomic::{AtomicBool, Ordering}, mpsc::{self, RecvTimeoutError}, Arc, Mutex}, thread, time::{Duration, Instant}};

use glam::DVec3;
use graphite_anvil::ChunkCoord;
use graphite_binary::nbt::EncodedNBT;
use graphite_concierge::LoginInformation;
use graphite_core_server::{inventory::{container::Container, inventory_slot::InventorySlot, menu::{BaseMenu, ContainerAction, ShouldReopen}, player_container_view::PlayerFunc}, player::{Player, PlayerExtension}, world::{World, WorldController, WorldExtension}};
use graphite_mc_constants::builtin::{Attribute, ContainerType};
use graphite_mc_protocol::types::{text::IntoTextComponent, ItemStack};
use graphite_network::{ConnectionSender, PacketBuffer, SendableConnection};

struct MyInventory {
}

#[allow(unused)]
impl Container for MyInventory {
    type Item = ItemStack;

    fn take(&mut self, slot: graphite_core_server::inventory::inventory_slot::InventorySlot) -> Option<Self::Item> {
        None
    }

    fn insert(&mut self, slot: graphite_core_server::inventory::inventory_slot::InventorySlot, item: Self::Item) {
    }

    fn get(&self, slot: graphite_core_server::inventory::inventory_slot::InventorySlot) -> Option<&Self::Item> {
        None
    }

    fn is_item_compatible_with(&self, item: &Self::Item, slot: graphite_core_server::inventory::inventory_slot::InventorySlot) -> bool {
        true
    }
}

struct MyMenu {
}

#[allow(unused)]
impl BaseMenu<MyPlayer> for MyMenu {
    fn container_type(&self) -> ContainerType {
        ContainerType::Generic9X6
    }

    fn title(&self) -> EncodedNBT {
        "Hello".into_text_component().to_encoded_nbt()
    }

    fn send_extra_open_packets(&self,container_id:i32, packet_buffer: &mut PacketBuffer) {
    }

    fn get_item_stack(&self,inventory: &MyInventory, slot:InventorySlot) -> ItemStack {
        ItemStack::EMPTY
    }

    fn do_container_action(&mut self,inventory: &mut MyInventory, action:ContainerAction) -> PlayerFunc<MyPlayer> {
        None
    }

    fn closed(self,inventory: &mut MyInventory, by_player:bool) -> PlayerFunc<MyPlayer> {
        None
    }
    
    fn synchronize_tick(&mut self,container_id:i32,packet_buffer: &mut graphite_network::PacketBuffer) -> ShouldReopen {
        ShouldReopen::No
    }
    
    fn on_synchronize_slot(&mut self,packet_buffer: &mut PacketBuffer,slot:InventorySlot){}
}

struct MyPlayer {

}

#[allow(unused)]
impl PlayerExtension for MyPlayer {
    type World = MyWorld;
    
    type InventoryContainer = MyInventory;
    
    type Menu = MyMenu;
    
    type ExtraTransferData = ();
    
    fn get_extra_transfer_data(player: &mut Player<Self>) -> Option<Self::ExtraTransferData> {
        None
    }
}

struct MyWorld {

}

#[allow(unused)]
impl WorldExtension for MyWorld {
    const VIEW_DISTANCE: u8 = 16;
    
    type JoinData = LoginInformation;
    
    fn tick(world: &mut graphite_core_server::world::World<Self>) {
    }
    
    fn on_player_join(world: &mut graphite_core_server::world::World<Self>, data: Self::JoinData, connection: std::rc::Rc<std::cell::RefCell<graphite_network::Connection>>) {
        let player = world.spawn_player(DVec3::new(128.0, 128.0, 128.0), 0.0, 0.0,
            connection, data.profile, MyInventory {}, MyPlayer {});
        let movement_speed = player.attributes.get(Attribute::MovementSpeed);
        println!("{:?}", movement_speed);

    }
}

pub struct TransferringPlayer {
    connection: SendableConnection,
    data: LoginInformation
}

struct MyUniverse {
    world: WorldController<MyWorld>,
    terminate: Arc<AtomicBool>,
    receiver: mpsc::Receiver<(SendableConnection, LoginInformation)>,
    players: Vec<TransferringPlayer>
}

impl MyUniverse {
    pub fn new() -> (Self, ConnectionSender<LoginInformation>) {
        let (tx, rx) = std::sync::mpsc::channel();
    
        let sender = ConnectionSender {
            inner: tx,
            waker: None,
        };

        let anvil_world = graphite_anvil::load_anvil_world(ChunkCoord::new(0, 0), 24,
            ChunkCoord::new(32, 32), &include_dir::include_dir!("src/world"), true).unwrap();



        let world = World::start(move || {
            let extension = MyWorld {};
            Box::new(World::new(extension, anvil_world.into()))
        });

        let universe = MyUniverse {
            world,
            terminate: Arc::new(AtomicBool::new(false)),
            receiver: rx,
            players: Vec::new()
        };

        (universe, sender)
    }

    pub fn tick(&mut self) {
        // Tick
    }

    pub fn start(&mut self) {
        let tick_rate = Duration::from_millis(50);
        let mut next_tick = Instant::now().checked_add(tick_rate).unwrap();

        while !self.terminate.load(Ordering::Relaxed) {
            let now = Instant::now();

            // Example tick loop
            let since = now.checked_duration_since(next_tick);
            if let Some(elapsed) = since {
                let mut tick_count = (elapsed.as_millis() / tick_rate.as_millis()).max(1);

                if tick_count > 0 {
                    if tick_count > 100 {
                        println!("Universe can't keep up, running {} ticks behind", tick_count-100);
                        tick_count = 100;
                    }

                    for _ in 0..tick_count {
                        self.tick();
                    }

                    next_tick = next_tick.checked_add(tick_rate * tick_count as u32).unwrap();
                }
            }

            let timeout = next_tick.checked_duration_since(now).unwrap_or(tick_rate);

            // Handle new connections
            match self.receiver.recv_timeout(timeout) {
                Ok((connection, data)) => {
                    self.world.join.send(connection, data);
                },
                Err(RecvTimeoutError::Timeout) => {
                    continue;
                },
                Err(RecvTimeoutError::Disconnected) => {
                    println!("disconnected? what do we do here? shutdown?")
                }
            }
        }
    }
}

fn main() {
    println!("Starting concierge on port 25565!");

    let (mut universe, mut core_sender) = MyUniverse::new();

    let sender = Box::new(move |login_information: LoginInformation, conn: SendableConnection| {
        core_sender.send(conn, login_information);
    });

    thread::spawn(move || {
        graphite_concierge::listen("0.0.0.0:25565", sender, Arc::new(Mutex::new(
            r#"{
                "version": {
                    "name": "1.21.5",
                    "protocol": 770
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
        )), graphite_concierge::AuthenticationMode::None);
    });

    println!("Starting core server");

    let terminate = universe.terminate.clone();

    ctrlc::set_handler(move || {
        terminate.store(true, Ordering::Relaxed);
    }).expect("Error setting ctrl-c handler");

    universe.start();
}
