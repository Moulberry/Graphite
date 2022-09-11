use anyhow::bail;
use graphite_command::dispatcher::RootDispatchNode;
use graphite_net::network_buffer::WriteBuffer;
use graphite_net::network_handler::{
    ConnectionSlab, NetworkManagerService, NewConnectionAccepter, UninitializedConnection,
};
use graphite_mc_protocol::types::GameProfile;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::{sync::mpsc::Sender, time::Duration};

use graphite_mc_protocol::play::server::{Commands, CustomPayload};

use crate::player::player_connection::{AbstractConnectionReference, PlayerConnection};
use crate::player::proto_player::ProtoPlayer;

// user defined universe service trait

pub trait UniverseService
where
    Self: Sized + 'static,
{
    // todo: use default associated type of `ConnectionReference<Self>`
    type ConnectionReferenceType: AbstractConnectionReference<Self>;

    fn handle_player_join(universe: &mut Universe<Self>, proto_player: ProtoPlayer<Self>);
    fn initialize(universe: &Universe<Self>);

    fn tick(universe: &mut Universe<Self>);
    fn get_player_count(universe: &Universe<Self>) -> usize;
}

// graphite universe

#[derive(Debug, PartialEq, Eq, Hash, Copy, Clone)]
#[repr(transparent)]
pub struct EntityId(i32);

impl EntityId {
    pub fn as_i32(&self) -> i32 {
        self.0
    }
}

pub struct Universe<U: UniverseService> {
    pub service: U,
    player_receiver: Receiver<(UninitializedConnection, GameProfile)>,
    entity_id_counter: i32,
    pub(crate) root_dispatch_node: Option<RootDispatchNode>,
    pub(crate) command_packet: Option<Commands>,
}

// graphite universe impl

impl<U: UniverseService> Universe<U> {
    pub fn handle_player_connect(
        &mut self,
        connection_ref: U::ConnectionReferenceType,
        profile: GameProfile,
    ) {
        let proto_player = ProtoPlayer::new(connection_ref, profile, self.new_entity_id());
        U::handle_player_join(self, proto_player);
    }

    pub(crate) fn write_brand_packet(
        &mut self,
        write_buffer: &mut WriteBuffer,
    ) -> anyhow::Result<()> {
        let brand_packet = CustomPayload {
            channel: "minecraft:brand",
            data: b"\x08Graphite",
        };
        graphite_net::packet_helper::write_packet(write_buffer, &brand_packet)
    }

    pub fn new_entity_id(&mut self) -> EntityId {
        self.entity_id_counter = self.entity_id_counter.wrapping_add(1);
        EntityId(self.entity_id_counter)
    }
}

// network service impl

impl<U: UniverseService> NetworkManagerService for Universe<U> {
    const TICK_RATE: Option<std::time::Duration> = Some(Duration::from_millis(50));
    type ConnectionServiceType = PlayerConnection<U>;

    fn new_connection_service(&mut self) -> PlayerConnection<U> {
        unimplemented!();
    }

    fn tick(
        &mut self,
        connections: &mut ConnectionSlab<Self>,
        accepter: NewConnectionAccepter<Self>,
    ) -> anyhow::Result<()> {
        // Accept pending connections
        loop {
            match self.player_receiver.try_recv() {
                Ok(received) => {
                    let connection = received.0;

                    let connection_index = accepter.accept_and_get_index(
                        connection,
                        PlayerConnection::new(),
                        connections,
                    )?;
                    let connection_ref = U::ConnectionReferenceType::new_from_connection(
                        connections,
                        connection_index,
                    );
                    self.handle_player_connect(connection_ref, received.1);
                }
                Err(err) if err == TryRecvError::Disconnected => {
                    if U::get_player_count(self) == 0 {
                        bail!("empty universe");
                    } else {
                        break;
                    }
                }
                Err(_) => {
                    break;
                }
            }
        }

        U::tick(self);

        Ok(())
    }
}

impl<U: UniverseService> Universe<U> {
    pub fn create_dummy(service: U) -> Universe<U> {
        let (_, rx) = mpsc::channel::<(UninitializedConnection, GameProfile)>();

        Universe {
            service,
            player_receiver: rx,
            entity_id_counter: 0,
            root_dispatch_node: None,
            command_packet: None,
        }
    }

    pub fn create_and_start<F: FnOnce() -> U + std::marker::Send + 'static>(
        service_func: F,
        commands: Option<(RootDispatchNode, Commands)>,
    ) -> Sender<(UninitializedConnection, GameProfile)> {
        let (tx, rx) = mpsc::channel::<(UninitializedConnection, GameProfile)>();

        std::thread::spawn(|| {
            let (root_dispatch_node, command_packet) = if let Some(commands) = commands {
                (Some(commands.0), Some(commands.1))
            } else {
                (None, None)
            };

            let service = service_func();
            let universe = Universe {
                service,
                player_receiver: rx,
                entity_id_counter: 0,
                root_dispatch_node,
                command_packet,
            };

            graphite_net::network_handler::start_with_init(universe, None, |network_manager| {
                U::initialize(&network_manager.service);
            })
            .unwrap();
        });

        tx
    }
}
