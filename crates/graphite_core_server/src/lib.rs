use std::{pin::Pin, rc::Rc, cell::{UnsafeCell, RefCell}, borrow::Cow};

use graphite_binary::nbt;
use graphite_concierge::LoginInformation;
use graphite_mc_protocol::{configuration::{self, serverbound::PacketHandler}, play::{self, clientbound::GameEventType}};
use graphite_network::{NetworkHandlerService, Connection, NetworkHandler, TcpStreamSender, FramedPacketHandler, HandleAction, PacketBuffer};
use registry::Registries;
use slab::Slab;

pub mod registry;

pub struct CoreServer {
    configuring_players: Slab<Rc<UnsafeCell<ConfiguringPlayer>>>,
}

pub struct ConfiguringPlayer {
    connection: Rc<RefCell<Connection>>,
    packet_buffer: PacketBuffer,
    idx: Option<usize>
}

impl FramedPacketHandler for ConfiguringPlayer {
    fn handle(&mut self, data: &[u8]) -> HandleAction {
        self.parse_and_handle(data).unwrap();
        HandleAction::Continue
    }

    fn disconnected(&mut self) {
        todo!()
    }
}

impl graphite_mc_protocol::configuration::serverbound::PacketHandler for ConfiguringPlayer {
    fn handle_finish_configuration(&mut self, _: configuration::serverbound::FinishConfiguration) -> anyhow::Result<()> {
        // send join game
        let join_game = play::clientbound::JoinGame {
            entity_id: 0,
            is_hardcore: false,
            dimension_names: vec!["graphite:default_world"],
            max_players: 100,
            view_distance: 8,
            simulation_distance: 8,
            reduced_debug_info: false,
            enable_respawn_screen: false,
            do_limited_crafting: false,
            dimension_type: "graphite:default_dimension_type",
            dimension_name: "graphite:default_world",
            hashed_seed: 0,
            gamemode: 1,
            previous_gamemode: -1,
            is_debug: false,
            is_flat: false,
            death_location: None,
            portal_cooldown: 0,
        };
        self.packet_buffer.write_packet(&join_game).unwrap();

        // send teleport
        self.packet_buffer.write_packet(&play::clientbound::PlayerPosition {
            x: 0.0,
            y: 400.0,
            z: 0.0,
            yaw: 0.0,
            pitch: 0.0,
            relative_arguments: 0,
            id: 0
        }).unwrap();

        // send StartWaitingForLevelChunks game event
        self.packet_buffer.write_packet(&play::clientbound::GameEvent {
            event_type: GameEventType::StartWaitingForLevelChunks,
            param: 0.0,
        }).unwrap();

        // send all packets
        self.connection.borrow_mut().send(self.packet_buffer.pop_written());

        Ok(())   
    }
}

impl NetworkHandlerService for Pin<Box<CoreServer>> {
    const MAXIMUM_PACKET_SIZE: usize = 2097151;

    type ExtraData = LoginInformation;

    fn accept_new_connection(&mut self, extra_data: Self::ExtraData, connection: Rc<RefCell<Connection>>) {
        let configuring_player = ConfiguringPlayer {
            connection: connection.clone(),
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
}

pub fn get_default_registry() -> nbt::NBT {
    let registries = Registries::default();
    registries.to_nbt()
}

impl CoreServer {
    pub fn new() -> (NetworkHandler<Pin<Box<CoreServer>>>, TcpStreamSender<LoginInformation>) {
        let core_server = CoreServer {
            configuring_players: Slab::new()
        };

        graphite_network::NetworkHandler::new_channel(
            Box::pin(core_server)
        ).unwrap()
    }
}