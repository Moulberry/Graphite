use std::time::Duration;

use anyhow::bail;
use binary::slice_serialization;
use binary::slice_serialization::SliceSerializable;
use bytes::BufMut;
use net::{
    network_handler::{
        ByteSender, ConnectionService, NetworkManagerService,
    },
    packet_helper::PacketReadResult,
    ConnectionState,
};
use protocol::{
    handshake::client::Handshake,
    play::server::{
        ChunkBlockData, ChunkDataAndUpdateLight, ChunkLightData, JoinGame, PlayerPositionAndLook,
        PluginMessage, UpdateViewPosition,
    },
    status::server::Response,
};
use rand::Rng;

struct ProtoPlayer {
    connection_state: ConnectionState,
}

impl<T: ConciergeService> ConnectionService<Concierge<T>> for ProtoPlayer {
    fn receive(
        &mut self,
        service: &mut Concierge<T>,
        bytes: &mut &[u8],
        byte_sender: &mut ByteSender,
    ) -> anyhow::Result<()> {
        loop {
            let packet_read_result = net::packet_helper::try_read_packet(bytes)?;
            match packet_read_result {
                PacketReadResult::Complete(bytes) => {
                    println!("Request: {:x?}", bytes);
                    service.process_framed_packet(self, byte_sender, bytes)?;
                }
                PacketReadResult::Partial => return Ok(()),
                PacketReadResult::Empty => return Ok(()),
            }
        }
    }
}

pub struct Concierge<T: ConciergeService + 'static> {
    serverlist_response: String,
    service: T,
}

impl<'a, T: ConciergeService + 'static> NetworkManagerService<ProtoPlayer> for Concierge<T> {
    const TICK_RATE: Option<Duration> = Some(Duration::from_secs(10));

    fn new_connection_service(&mut self) -> ProtoPlayer {
        ProtoPlayer {
            connection_state: ConnectionState::Handshake,
        }
    }

    fn tick(&mut self) {
        self.serverlist_response = self.service.get_serverlist_response();
    }
}

impl<'a, T: ConciergeService + 'static> Concierge<T> {
    pub fn bind(addr: &str, mut service: T) -> anyhow::Result<()> {
        let concierge = Concierge {
            serverlist_response: service.get_serverlist_response(),
            service
        };

        net::network_handler::start(concierge, addr)?;

        Ok(())
    }

    fn process_framed_packet(
        &mut self,
        protoplayer: &mut ProtoPlayer,
        byte_sender: &mut ByteSender,
        bytes: &[u8],
    ) -> anyhow::Result<()> {
        match protoplayer.connection_state {
            net::ConnectionState::Handshake => {
                if bytes.len() < 3 {
                    bail!("insufficient bytes for handshake");
                } else if bytes[0..3] == [0xFE, 0x01, 0xFA] {
                    bail!("legacy server list ping from 2013 is not supported");
                } else {
                    // Handshake: https://wiki.vg/Protocol#Handshake
                    let mut bytes = bytes;
    
                    let packet_id_byte: u8 = slice_serialization::VarInt::read(&mut bytes)?.try_into()?;
    
                    if let Ok(packet_id) =
                        protocol::handshake::client::PacketId::try_from(packet_id_byte)
                    {
                        println!("got packet by id: {:?}", packet_id);
    
                        let handshake_packet = Handshake::read(&mut bytes)?;
                        slice_serialization::check_empty(bytes)?;
    
                        protoplayer.connection_state = match handshake_packet.next_state {
                            1 => net::ConnectionState::Status,
                            2 => net::ConnectionState::Login,
                            next => bail!("unknown next state {} for ClientHandshake", next),
                        };
                    } else {
                        bail!(
                            "unknown packet_id {} during {:?}",
                            packet_id_byte,
                            protoplayer.connection_state
                        );
                    }
                }
            }
            net::ConnectionState::Status => {
                // Server List Ping: https://wiki.vg/Server_List_Ping
                let mut bytes = bytes;
    
                let packet_id = slice_serialization::VarInt::read(&mut bytes)?;
                match packet_id {
                    0 => {
                        let server_response = Response {
                            json: &self.serverlist_response,
                        };
                        net::packet_helper::send_packet(byte_sender, &server_response)?;
                    },
                    1 => {
                        if bytes.len() == 8 {
                            // todo: should probably make this an actual packet, even if its slightly slower
                            // length = 9, packet = 1, rest is copied over from `bytes`
                            let mut response: [u8; 10] = [9, 1, 0, 0, 0, 0, 0, 0, 0, 0];
                            response[2..].clone_from_slice(bytes);
    
                            byte_sender.send(Box::from(response));
                        }
    
                        // protoplayer.close();
                    }
                    _ => bail!(
                        "unknown packet_id {} during {:?}",
                        packet_id,
                        protoplayer.connection_state
                    ),
                }
    
                return Ok(());
            }
            net::ConnectionState::Login => {
                let mut bytes = bytes;
    
                let packet_id_byte: u8 = slice_serialization::VarInt::read(&mut bytes)?.try_into()?;
    
                if let Ok(packet_id) = protocol::login::client::PacketId::try_from(packet_id_byte) {
                    println!("login - got packet by id: {:?}", packet_id);
    
                    match packet_id {
                        protocol::login::client::PacketId::LoginStart => {
                            let login_start_packet =
                                protocol::login::client::LoginStart::read(&mut bytes)?;
                            slice_serialization::check_empty(bytes)?;
    
                            println!("logging in with username: {}", login_start_packet.username);
    
                            std::thread::sleep(std::time::Duration::from_millis(100));
    
                            let login_success_packet = protocol::login::server::LoginSuccess {
                                uuid: rand::thread_rng().gen(),
                                username: login_start_packet.username,
                                property_count: 0,
                            };
    
                            println!("sending login success!");
    
                            net::packet_helper::send_packet(byte_sender, &login_success_packet)?;
    
                            protoplayer.connection_state = net::ConnectionState::Play;
    
                            // fake play, for testing
    
                            std::thread::sleep(std::time::Duration::from_millis(100));
    
                            let registry_codec = quartz_nbt::snbt::parse(include_str!(
                                "../../assets/registry_codec.json"
                            ))?;
                            let mut binary: Vec<u8> = Vec::new();
                            quartz_nbt::io::write_nbt(
                                &mut binary,
                                None,
                                &registry_codec,
                                quartz_nbt::io::Flavor::Uncompressed,
                            )?;
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
                            net::packet_helper::send_packet(byte_sender, &join_game_packet)?;
    
                            // Brand
                            let brand_packet = PluginMessage {
                                channel: "minecraft:brand",
                                data: b"\x08Graphite",
                            };
                            net::packet_helper::send_packet(byte_sender, &brand_packet)?;
    
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
                            )?;
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
                                    net::packet_helper::send_packet(byte_sender, &chunk_packet)?;
                                }
                            }
    
                            // Update view position
                            let update_view_position_packet = UpdateViewPosition {
                                chunk_x: 0,
                                chunk_z: 0,
                            };
                            net::packet_helper::send_packet(byte_sender, &update_view_position_packet)?;
    
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
                            net::packet_helper::send_packet(byte_sender, &position_packet)?;
                        }
                    }
                } else {
                    bail!(
                        "unknown packet_id {} during {:?}",
                        packet_id_byte,
                        protoplayer.connection_state
                    );
                }
            }
            net::ConnectionState::Play => {
                let mut bytes = bytes;
    
                let packet_id_byte: u8 = slice_serialization::VarInt::read(&mut bytes)?.try_into()?;
    
                println!("got play packet: {:?}", packet_id_byte);
            }
        }
    
        Ok(())
    }
}

pub trait ConciergeService {
    fn get_serverlist_response(&mut self) -> String;
}