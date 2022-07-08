use std::collections::VecDeque;
use std::{time::Duration, sync::mpsc::Sender};
use std::sync::mpsc::{self, Receiver};
use io_uring::{SubmissionQueue, squeue};
use net::network_handler::{NetworkManagerService, ConnectionService, Connection, ByteSender};

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
struct Player {
    player_id: u16,
    universe: *mut Universe
}

impl Player {
    fn get_universe(&self) -> &mut Universe {
        unsafe { self.universe.as_mut().unwrap() }
    }

    fn remove_from_world(&mut self) {
        unsafe { self.universe.as_mut() }.unwrap().remove_player(self);
        self.universe = std::ptr::null_mut();

    }
}

fn bad_code() -> u32 {
    /*let mut slab: Slab<u32> = Default::default();

    let five = slab.insert(5);
    let five_ref = slab.get(five).unwrap();
    slab.remove(five);

    *five_ref*/3
}

fn my_code() {
    let player = Player {
        player_id: 0,
        universe: std::ptr::null_mut()
    };

    let mut universe = Universe {
        players: Default::default()
    };

    universe.players.insert(player);
    let player = universe.players.get_mut(0).unwrap();

    let universe = player.get_universe();
    //let player = universe.players.get_mut(0).unwrap();

    let player_id = player.player_id;

    player.remove_from_world();

    //universe.remove_player(player);

    //player.get_universe();

    //universe.remove_player(player);
    
    //std::mem::drop(player);
    //println!("{:?}", universe.players);

    //drop(player);
    //universe.remove_player(player);
}

pub struct Universe {
    players: Slab<Player>
}

impl Universe {
    fn remove_player(&mut self, player: &mut Player) {
        self.players.remove(player.player_id as _);
    }
}

impl ConnectionService<UniverseNetworkManagerService> for Player {
    const BUFFER_SIZE: u32 = 4_194_304;

    fn on_receive(&mut self, service: &mut UniverseNetworkManagerService, bytes: &mut &[u8], byte_sender: net::network_handler::ByteSender) -> anyhow::Result<bool> {
        let packet_read_result = net::packet_helper::try_read_packet(bytes)?;

        match packet_read_result {
            PacketReadResult::Complete(bytes) => {
                println!("Request: {:x?}", bytes);
                /*let should_consume = service.process_framed_packet(self, &mut byte_sender, bytes)?;
                if should_consume {
                    return Ok(true)
                }*/
            }
            PacketReadResult::Partial => return Ok(false),
            PacketReadResult::Empty => return Ok(false),
        }

        println!("received some data!");

        Ok(false)
    }

    fn on_created(&mut self, _: ByteSender) {
        
    }
}

type PlayerSender = Sender<Connection<concierge::ProtoPlayer>>;

struct UniverseNetworkManagerService {
    player_receiver: Receiver<Connection<concierge::ProtoPlayer>>
}

impl NetworkManagerService<Player> for UniverseNetworkManagerService {
    const TICK_RATE: Option<std::time::Duration> = Some(Duration::from_millis(50));

    fn new_connection_service(&mut self) -> Player {
        unimplemented!();
    }

    fn consume_connection(&mut self, _: net::network_handler::Connection<Player>) {
        todo!()
    }

    fn tick(&mut self, connections: &mut Slab<Connection<Player>>, mut sq: SubmissionQueue, backlog: &mut VecDeque<squeue::Entry>) {
        // Accept pending connections
        while let Ok(connection) = self.player_receiver.try_recv() {
            //let proto_player = connection.service;

            let connection = connection.with_service(self.new_connection_service());
            Self::take_connection(connection, connections, &mut sq, backlog);
        }
    }
}

pub fn create_and_start() -> PlayerSender {
    let (rx, tx) = mpsc::channel::<Connection<concierge::ProtoPlayer>>();

    std::thread::spawn(|| {
        net::network_handler::start(UniverseNetworkManagerService {
            player_receiver: tx
        }, None).unwrap();
    });

    rx
} 


/*

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
        net::packet_helper::send_packet(&mut byte_sender, &join_game_packet).unwrap();

        // Brand
        let brand_packet = PluginMessage {
            channel: "minecraft:brand",
            data: b"\x08Graphite",
        };
        net::packet_helper::send_packet(&mut byte_sender, &brand_packet).unwrap();

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
                net::packet_helper::send_packet(&mut byte_sender, &chunk_packet).unwrap();
            }
        }

        // Update view position
        let update_view_position_packet = UpdateViewPosition {
            chunk_x: 0,
            chunk_z: 0,
        };
        net::packet_helper::send_packet(&mut byte_sender, &update_view_position_packet).unwrap();

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
        net::packet_helper::send_packet(&mut byte_sender, &position_packet).unwrap();


*/