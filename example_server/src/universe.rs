use anyhow::bail;
use net::network_buffer::WriteBuffer;
use net::network_handler::{
    Connection, NetworkManagerService, NewConnectionAccepter, UninitializedConnection,
};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::{sync::mpsc::Sender, time::Duration};

use crate::proto_player::ProtoPlayer;
use protocol::play::server::PluginMessage;
use slab::Slab;

use crate::player_connection::PlayerConnection;

// user defined universe service trait

pub trait UniverseService
where
    Self: Sized,
{
    fn handle_player_join(universe: &mut Universe<Self>, proto_player: ProtoPlayer<Self>);
    fn initialize(universe: &mut Universe<Self>);

    fn tick(universe: &mut Universe<Self>);
    fn get_player_count(universe: &mut Universe<Self>) -> usize;
}

// graphite universe

pub struct Universe<U: UniverseService> {
    pub service: U,
    player_receiver: Receiver<UninitializedConnection>,
}

// graphite universe impl

impl<U: UniverseService> Universe<U> {
    fn handle_player_connect(
        &mut self,
        connection_ptr: (*mut Connection<Universe<U>>, *mut PlayerConnection<U>),
    ) {
        let proto_player = ProtoPlayer::new(connection_ptr);
        U::handle_player_join(self, proto_player);
    }

    pub(crate) fn write_brand_packet(
        &mut self,
        write_buffer: &mut WriteBuffer,
    ) -> anyhow::Result<()> {
        let brand_packet = PluginMessage {
            channel: "minecraft:brand",
            data: b"\x08Graphite",
        };
        net::packet_helper::write_packet(write_buffer, &brand_packet)
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
        connections: &mut Slab<(Connection<Self>, Box<Self::ConnectionServiceType>)>,
        accepter: NewConnectionAccepter<Self>,
    ) -> anyhow::Result<()> {
        // Accept pending connections
        loop {
            match self.player_receiver.try_recv() {
                Ok(connection) => {
                    println!("got new connection!");
                    let accept_result = unsafe {
                        accepter.accept_and_get_ptr(
                            connection,
                            PlayerConnection::new(),
                            connections,
                        )
                    };
                    if let Some(connection_ptr) = accept_result {
                        self.handle_player_connect(connection_ptr);
                    }
                }
                Err(err) if err == TryRecvError::Disconnected => {
                    if U::get_player_count(self) == 0 {
                        println!("emptying universe!!!");
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

pub fn create_and_start<U: UniverseService, F: FnOnce() -> U + std::marker::Send + 'static>(
    service_func: F,
) -> Sender<UninitializedConnection> {
    let (rx, tx) = mpsc::channel::<UninitializedConnection>();

    std::thread::spawn(|| {
        let service = service_func();
        let mut universe = Universe {
            service,
            player_receiver: tx,
        };

        U::initialize(&mut universe);

        let _ = net::network_handler::start(universe, None);
    });

    rx
}

// fn send_initial_packets_for_testing(connection: &mut Connection<Universe>) {
//     let mut write_buffer = WriteBuffer::new();

//     use protocol::play::server::ChunkBlockData;
//     use protocol::play::server::ChunkDataAndUpdateLight;
//     use protocol::play::server::ChunkLightData;
//     use protocol::play::server::JoinGame;
//     use protocol::play::server::PlayerPositionAndLook;
//     use protocol::play::server::PluginMessage;
//     use protocol::play::server::UpdateViewPosition;
//     use bytes::BufMut;

//     let registry_codec = quartz_nbt::snbt::parse(include_str!(
//         "../../assets/registry_codec.json"
//     )).unwrap();
//     let mut binary: Vec<u8> = Vec::new();
//     quartz_nbt::io::write_nbt(
//         &mut binary,
//         None,
//         &registry_codec,
//         quartz_nbt::io::Flavor::Uncompressed,
//     ).unwrap();
//     binary.shrink_to_fit();

//     // Join Game
//     let join_game_packet = JoinGame {
//         entity_id: 0,
//         is_hardcore: false,
//         gamemode: 1,
//         previous_gamemode: -1,
//         dimension_names: vec!["minecraft:overworld"],
//         registry_codec: &binary,
//         dimension_type: "minecraft:overworld",
//         dimension_name: "minecraft:overworld",
//         hashed_seed: 69,
//         max_players: 100,
//         view_distance: 8,
//         simulation_distance: 8,
//         reduced_debug_info: false,
//         enable_respawn_screen: false,
//         is_debug: false,
//         is_flat: false,
//         has_death_location: false,
//     };
//     net::packet_helper::write_packet(&mut write_buffer, &join_game_packet).unwrap();

//     // Brand
//     let brand_packet = PluginMessage {
//         channel: "minecraft:brand",
//         data: b"\x08Graphite",
//     };
//     net::packet_helper::write_packet(&mut write_buffer, &brand_packet).unwrap();

//     let mut heightmap_nbt = quartz_nbt::NbtCompound::new();
//     let mut motion_blocking_nbt = quartz_nbt::NbtList::new();
//     for _ in 0..256 {
//         motion_blocking_nbt.push(0_i64);
//     }
//     heightmap_nbt.insert("MOTION_BLOCKING", motion_blocking_nbt);

//     let mut binary: Vec<u8> = Vec::new();
//     quartz_nbt::io::write_nbt(
//         &mut binary,
//         None,
//         &heightmap_nbt,
//         quartz_nbt::io::Flavor::Uncompressed,
//     ).unwrap();
//     binary.shrink_to_fit();

//     // Chunk
//     for x in -5..5 {
//         for z in -5..5 {
//             let mut chunk_data = vec![0_u8; 0];
//             for i in 0..24 {
//                 chunk_data.put_i16(16 * 16 * 16); // block count

//                 // blocks
//                 chunk_data.put_u8(0); // single pallete, 0 bits per entry
//                 if i < 18 && x + z != 0 {
//                     chunk_data.put_u8(1); // palette. stone
//                 } else {
//                     chunk_data.put_u8(0); // palette. air
//                 }
//                 chunk_data.put_u8(0); // 0 size array

//                 // biomes
//                 chunk_data.put_u8(0); // single pallete, 0 bits per entry
//                 chunk_data.put_u8(1); // some biome
//                 chunk_data.put_u8(0); // 0 size array
//             }

//             let chunk_packet = ChunkDataAndUpdateLight {
//                 chunk_x: x,
//                 chunk_z: z,
//                 chunk_block_data: ChunkBlockData {
//                     heightmaps: &binary,
//                     data: &chunk_data,
//                     block_entity_count: 0,
//                     trust_edges: false,
//                 },
//                 chunk_light_data: ChunkLightData {
//                     sky_light_mask: vec![],
//                     block_light_mask: vec![],
//                     empty_sky_light_mask: vec![],
//                     empty_block_light_mask: vec![],
//                     sky_light_entries: vec![],
//                     block_light_entries: vec![],
//                 },
//             };
//             net::packet_helper::write_packet(&mut write_buffer, &chunk_packet).unwrap();
//         }
//     }

//     // Update view position
//     let update_view_position_packet = UpdateViewPosition {
//         chunk_x: 0,
//         chunk_z: 0,
//     };
//     net::packet_helper::write_packet(&mut write_buffer, &update_view_position_packet).unwrap();

//     // Position
//     let position_packet = PlayerPositionAndLook {
//         x: 0.0,
//         y: 500.0,
//         z: 0.0,
//         yaw: 15.0,
//         pitch: 0.0,
//         flags: 0,
//         teleport_id: 0,
//         dismount_vehicle: false,
//     };
//     net::packet_helper::write_packet(&mut write_buffer, &position_packet).unwrap();

//     connection.write(write_buffer.get_written());
// }
