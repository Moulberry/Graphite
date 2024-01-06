use std::{pin::Pin, rc::Rc, cell::{UnsafeCell, RefCell}, borrow::Cow, ptr::NonNull, time::Duration};

use graphite_binary::nbt;
use graphite_concierge::LoginInformation;
use graphite_mc_protocol::{configuration::{self, serverbound::PacketHandler}};
use graphite_network::{NetworkHandlerService, Connection, NetworkHandler, TcpStreamSender, FramedPacketHandler, HandleAction, PacketBuffer};
use registry::Registries;
use slab::Slab;
use world::{GenericWorld, WorldExtension, World, chunk_section::ChunkSection, ChunkList};
// use world::{World, WorldExtension};

pub mod registry;
pub mod world;
pub mod player;
pub mod entity;
pub mod types;
pub mod inventory;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Behaviour {
    Vanilla,
    Pass
}

pub trait UniverseExtension: Sized + Unpin + 'static {
    fn init(universe: &mut Universe<Self>) -> Self;
    fn spawn_player(universe: &mut Universe<Self>, player: ConfiguringPlayer<Self>);
}

#[derive(Copy, Clone, Debug)]
pub struct WorldId(usize);

pub struct Universe<U: UniverseExtension> {
    configuring_players: Slab<Rc<UnsafeCell<ConfiguringPlayer<U>>>>,
    worlds: Slab<Box<dyn GenericWorld>>,
    extension: Option<U>
}

pub struct ConfiguringPlayer<U: UniverseExtension> {
    connection: Rc<RefCell<Connection>>,
    universe: NonNull<Universe<U>>,
    packet_buffer: PacketBuffer,
    idx: Option<usize>
}

impl <T: UniverseExtension> Drop for ConfiguringPlayer<T> {
    fn drop(&mut self) {
        if Rc::strong_count(&self.connection) <= 2 {
            self.connection.borrow_mut().disconnect();
        }
    }
}

impl <T: UniverseExtension> FramedPacketHandler for ConfiguringPlayer<T> {
    fn handle(&mut self, data: &[u8]) -> HandleAction {
        self.parse_and_handle(data).unwrap();
        HandleAction::Continue
    }

    fn disconnected(&mut self) {
        todo!()
    }
}

impl <T: UniverseExtension> graphite_mc_protocol::configuration::serverbound::PacketHandler for ConfiguringPlayer<T> {
    fn handle_finish_configuration(&mut self, _: configuration::serverbound::FinishConfiguration) -> anyhow::Result<()> {
        self.connection.borrow_mut().disconnect_handler();

        let universe = unsafe { self.universe.as_mut() };

        let configuring_player = universe.configuring_players.remove(self.idx.unwrap());
        let configuring_player = Rc::into_inner(configuring_player).unwrap();
        let mut configuring_player = UnsafeCell::into_inner(configuring_player);

        configuring_player.idx = None;
        T::spawn_player(universe, configuring_player);

        Ok(())   
    }
}

impl <T: UniverseExtension> NetworkHandlerService for Pin<Box<Universe<T>>> {
    const MAXIMUM_PACKET_SIZE: usize = 2097151;
    const TICK_RATE: Option<std::time::Duration> = Some(Duration::from_millis(50));

    type ExtraData = LoginInformation;

    fn accept_new_connection(&mut self, _: Self::ExtraData, connection: Rc<RefCell<Connection>>) {
        let configuring_player = ConfiguringPlayer {
            connection: connection.clone(),
            universe: self.as_mut().get_mut().into(),
            packet_buffer: PacketBuffer::new(),
            idx: None
        };
        let configuring_player = Rc::new(UnsafeCell::new(configuring_player));

        // Insert player
        let idx = self.configuring_players.insert(configuring_player.clone());

        // Update player idx
        let configuring_player_ref = unsafe { configuring_player.get().as_mut() }.unwrap();
        configuring_player_ref.idx = Some(idx);

        // Write registries
        configuring_player_ref.packet_buffer.write_packet(&configuration::clientbound::RegistryData {
            nbt: Cow::Owned(get_default_registry().into())
        }).unwrap();

        // Write end configuration
        configuring_player_ref.packet_buffer.write_packet(&configuration::clientbound::FinishConfiguration{}).unwrap();

        // Set connection handler & send packet
        let mut connection = connection.borrow_mut();
        connection.set_handler(configuring_player.clone());
        connection.send(configuring_player_ref.packet_buffer.pop_written());
    }

    fn tick(&mut self) {
        for (_, world) in &mut self.worlds {
            world.tick();
        }
    }
}

pub fn get_default_registry() -> nbt::NBT {
    let registries = Registries::default();
    registries.to_nbt()
}

impl <U: UniverseExtension> Universe<U> {
    pub fn new() -> (NetworkHandler<Pin<Box<Universe<U>>>>, TcpStreamSender<LoginInformation>) {
        let mut universe = Universe {
            configuring_players: Slab::new(),
            worlds: Slab::new(),
            extension: None
        };

        let extension = U::init(&mut universe);
        universe.extension = Some(extension);

        graphite_network::NetworkHandler::new_channel(
            Box::pin(universe)
        ).unwrap()
    }

    pub fn create_world<W: WorldExtension<Universe = U> + 'static>(&mut self, extension: W, chunks: ChunkList) -> WorldId {
        let world: World<W> = World::new(self, extension, chunks);
        let id = self.worlds.insert(Box::new(world));
        WorldId(id)
    }

    pub fn world<W: WorldExtension + 'static>(&mut self, world_id: WorldId) -> Option<&mut World<W>> {
        self.worlds.get_mut(world_id.0)?.downcast_mut::<World<W>>()
    }

    pub fn extension(&mut self) -> &mut U {
        self.extension.as_mut().unwrap()
    }
}