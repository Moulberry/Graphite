use std::collections::VecDeque;
use std::{time::Duration, sync::mpsc::Sender};
use std::sync::mpsc::{self, Receiver};
use io_uring::{SubmissionQueue, squeue};
use net::network_buffer::WriteBuffer;
use net::network_handler::{NetworkManagerService, ConnectionService, Connection, NewConnectionAccepter, UninitializedConnection};

use net::packet_helper::PacketReadResult;
use slab::Slab;

#[derive(Clone, Copy, Debug)]
struct Coordinate {
    x: f32,
    y: f32,
    z: f32,
    yaw: f32,
    pitch: f32
}

#[derive(Debug)]
struct PlayerConnection {
    
}

impl ConnectionService for PlayerConnection {
    const BUFFER_SIZE: u32 = 4_194_304;
    type NetworkManagerServiceType = UniverseNetworkManagerService;

    fn on_receive(&mut self, connection: &mut Connection<Self::NetworkManagerServiceType>, num_bytes: u32) -> anyhow::Result<u32> {
        let mut bytes = connection.read_bytes(num_bytes);
        let mut write_buffer: WriteBuffer = WriteBuffer::new();

        loop {
            let packet_read_result = net::packet_helper::try_read_packet(&mut bytes)?;
            match packet_read_result {
                PacketReadResult::Complete(bytes) => {
                    println!("Request: {:x?}", bytes);
                    //should_consume = self.process_framed_packet(&mut write_buffer, connection, bytes)?;
                }
                PacketReadResult::Partial => break,
                PacketReadResult::Empty => break,
            }
        }

        let bytes_remaining = bytes.len() as u32;

        let to_write = write_buffer.get_written();
        if to_write.len() > 0 {
            connection.write(to_write);
        }

        Ok(bytes_remaining)
    }
}

struct UniverseNetworkManagerService {
    player_receiver: Receiver<UninitializedConnection>
}

impl NetworkManagerService for UniverseNetworkManagerService {
    const TICK_RATE: Option<std::time::Duration> = Some(Duration::from_millis(50));
    type ConnectionServiceType = PlayerConnection;

    fn new_connection_service(&mut self) -> PlayerConnection {
        unimplemented!();
    }

    fn tick(&mut self, connections: &mut Slab<(Connection<Self>, Self::ConnectionServiceType)>, accepter: NewConnectionAccepter<Self>) {
        // Accept pending connections
        while let Ok(connection) = self.player_receiver.try_recv() {
            let accept_result = unsafe {
                accepter.accept_and_get_ptr(connection, PlayerConnection {}, connections)
            };
            if let Some(connection_ptr) = accept_result {
                send_initial_packets_for_testing(unsafe { connection_ptr.as_mut() }.unwrap());
            }
        }
    }
}

pub fn create_and_start() -> Sender<UninitializedConnection> {
    let (rx, tx) = mpsc::channel::<UninitializedConnection>();

    std::thread::spawn(|| {
        net::network_handler::start(UniverseNetworkManagerService {
            player_receiver: tx
        }, None).unwrap();
    });

    rx
} 

fn send_initial_packets_for_testing(connection: &mut Connection<UniverseNetworkManagerService>) {
    let mut write_buffer = WriteBuffer::new();

    use protocol::play::server::ChunkBlockData;
    use protocol::play::server::ChunkDataAndUpdateLight;
    use protocol::play::server::ChunkLightData;
    use protocol::play::server::JoinGame;
    use protocol::play::server::PlayerPositionAndLook;
    use protocol::play::server::PluginMessage;
    use protocol::play::server::UpdateViewPosition;
    use bytes::BufMut;

    let registry_codec = quartz_nbt::snbt::parse(include_str!(
        "../../assets/registry_codec.json"
    )).unwrap();
    let mut binary: Vec<u8> = Vec::new();
    quartz_nbt::io::write_nbt(
        &mut binary,
        None,
        &registry_codec,
        quartz_nbt::io::Flavor::Uncompressed,
    ).unwrap();
    binary.shrink_to_fit();

    // Join Game
    let join_game_packet = JoinGame {
        entity_id: 0,
        is_hardcore: false,
        gamemode: 1,
        previous_gamemode: -1,
        dimension_names: vec!["minecraft:overworld"],
        registry_codec: &binary,
        dimension_type: "minecraft:overworld",
        dimension_name: "minecraft:overworld",
        hashed_seed: 69,
        max_players: 100,
        view_distance: 8,
        simulation_distance: 8,
        reduced_debug_info: false,
        enable_respawn_screen: false,
        is_debug: false,
        is_flat: false,
        has_death_location: false,
    };
    net::packet_helper::write_packet(&mut write_buffer, &join_game_packet).unwrap();

    // Brand
    let brand_packet = PluginMessage {
        channel: "minecraft:brand",
        data: b"\x08Graphite",
    };
    net::packet_helper::write_packet(&mut write_buffer, &brand_packet).unwrap();

    let mut heightmap_nbt = quartz_nbt::NbtCompound::new();
    let mut motion_blocking_nbt = quartz_nbt::NbtList::new();
    for _ in 0..256 {
        motion_blocking_nbt.push(0_i64);
    }
    heightmap_nbt.insert("MOTION_BLOCKING", motion_blocking_nbt);

    let mut binary: Vec<u8> = Vec::new();
    quartz_nbt::io::write_nbt(
        &mut binary,
        None,
        &heightmap_nbt,
        quartz_nbt::io::Flavor::Uncompressed,
    ).unwrap();
    binary.shrink_to_fit();

    // Chunk
    for x in -5..5 {
        for z in -5..5 {
            let mut chunk_data = vec![0_u8; 0];
            for i in 0..24 {
                chunk_data.put_i16(16 * 16 * 16); // block count

                // blocks
                chunk_data.put_u8(0); // single pallete, 0 bits per entry
                if i < 18 && x + z != 0 {
                    chunk_data.put_u8(1); // palette. stone
                } else {
                    chunk_data.put_u8(0); // palette. air
                }
                chunk_data.put_u8(0); // 0 size array

                // biomes
                chunk_data.put_u8(0); // single pallete, 0 bits per entry
                chunk_data.put_u8(1); // some biome
                chunk_data.put_u8(0); // 0 size array
            }

            let chunk_packet = ChunkDataAndUpdateLight {
                chunk_x: x,
                chunk_z: z,
                chunk_block_data: ChunkBlockData {
                    heightmaps: &binary,
                    data: &chunk_data,
                    block_entity_count: 0,
                    trust_edges: false,
                },
                chunk_light_data: ChunkLightData {
                    sky_light_mask: vec![],
                    block_light_mask: vec![],
                    empty_sky_light_mask: vec![],
                    empty_block_light_mask: vec![],
                    sky_light_entries: vec![],
                    block_light_entries: vec![],
                },
            };
            net::packet_helper::write_packet(&mut write_buffer, &chunk_packet).unwrap();
        }
    }

    // Update view position
    let update_view_position_packet = UpdateViewPosition {
        chunk_x: 0,
        chunk_z: 0,
    };
    net::packet_helper::write_packet(&mut write_buffer, &update_view_position_packet).unwrap();

    // Position
    let position_packet = PlayerPositionAndLook {
        x: 0.0,
        y: 500.0,
        z: 0.0,
        yaw: 15.0,
        pitch: 0.0,
        flags: 0,
        teleport_id: 0,
        dismount_vehicle: false,
    };
    net::packet_helper::write_packet(&mut write_buffer, &position_packet).unwrap();

    connection.write(write_buffer.get_written());
}