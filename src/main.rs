use std::io::prelude::*;
use std::io::Cursor;
use std::net::TcpListener;
use std::net::TcpStream;

mod binary;
mod net;
mod packet;

use anyhow::bail;
use bytes::BufMut;

use crate::binary::slice_reader;
use crate::binary::slice_writer;
use crate::packet::handshake::client::Handshake;
use crate::packet::play::server::ChunkBlockData;
use crate::packet::play::server::ChunkDataAndUpdateLight;
use crate::packet::play::server::ChunkLightData;
use crate::packet::play::server::JoinGame;
use crate::packet::play::server::PlayerPositionAndLook;
use crate::packet::play::server::PluginMessage;
use crate::packet::play::server::UpdateViewPosition;
use crate::packet::status::server::Response;
use net::network_buffer::{PacketReadBuffer, PacketReadResult};
use rand::Rng;

#[derive(PartialEq, Eq)]
struct Uuid(u128);

#[derive(Debug)]
enum ConnectionState {
    Handshake,
    Status,
    Login,
    Play,
}

struct PlayerConnection {
    stream: TcpStream,
    state: ConnectionState,
    closed: bool,
}

fn main() {
    let listener = TcpListener::bind("127.0.0.1:25565").unwrap();

    //let map: HashMap<UUID, Player> = HashMap::new();

    for stream in listener.incoming() {
        let stream = stream.unwrap();

        let connection = PlayerConnection {
            stream,
            state: ConnectionState::Handshake,
            closed: false,
        };

        handle_connection(connection);
    }
}

impl PlayerConnection {
    pub fn close(&mut self) {
        if !self.closed {
            let _ = self.stream.shutdown(std::net::Shutdown::Both);
            self.closed = true;
        }
    }
}

fn handle_connection(mut connection: PlayerConnection) {
    let mut buffer = PacketReadBuffer::new();

    while !connection.closed {
        if buffer.read_all(&mut connection.stream).is_err() {
            // todo: maybe inform player of error via disconnect packet?
            connection.close();
        }

        while !connection.closed {
            if let Ok(packet_read_result) = buffer.try_read_packet() {
                match packet_read_result {
                    PacketReadResult::Complete(bytes) => {
                        println!("Request: {:?}", bytes);
                        if let Err(e) = process_framed_packet(&mut connection, bytes) {
                            println!("got error: {:?}", e);
                            // todo: maybe inform player of error via disconnect packet?
                            connection.close();
                        }
                    }
                    PacketReadResult::Partial(_bytes) => {
                        todo!();
                    }
                    PacketReadResult::Empty => break,
                }
            } else {
                // todo: maybe inform player of error via disconnect packet?
                connection.close();
            }
        }
    }
}

use binary::slice_serializable::SliceSerializable;

fn process_framed_packet(connection: &mut PlayerConnection, bytes: &[u8]) -> anyhow::Result<()> {
    match connection.state {
        ConnectionState::Handshake => {
            if bytes.len() < 3 {
                bail!("insufficient bytes for handshake");
            } else if bytes[0..3] == [0xFE, 0x01, 0xFA] {
                bail!("legacy server list ping from 2013 is not supported");
            } else {
                // Handshake: https://wiki.vg/Protocol#Handshake
                let mut bytes = bytes;

                let packet_id_byte: u8 =
                    binary::slice_reader::read_varint(&mut bytes)?.try_into()?;

                if let Ok(packet_id) = packet::handshake::client::PacketId::try_from(packet_id_byte)
                {
                    println!("got packet by id: {:?}", packet_id);

                    let handshake_packet = Handshake::read(&mut bytes)?;
                    slice_reader::ensure_fully_read(bytes)?;

                    connection.state = match handshake_packet.next_state {
                        1 => ConnectionState::Status,
                        2 => ConnectionState::Login,
                        next => bail!("unknown next state {} for ClientHandshake", next),
                    };
                } else {
                    bail!(
                        "unknown packet_id {} during {:?}",
                        packet_id_byte,
                        connection.state
                    );
                }

                return Ok(());
            }
        }
        ConnectionState::Status => {
            // Server List Ping: https://wiki.vg/Server_List_Ping
            let mut bytes = bytes;

            let packet_id = binary::slice_reader::read_varint(&mut bytes)?;
            match packet_id {
                0 => send_serverlist_response(&mut connection.stream)?,
                1 => {
                    if bytes.len() == 8 {
                        // todo: should probably make this an actual packet, even if its slightly slower
                        // length = 9, packet = 1, rest is copied over from `bytes`
                        let mut response: [u8; 10] = [9, 1, 0, 0, 0, 0, 0, 0, 0, 0];
                        response[2..].clone_from_slice(bytes);

                        connection.stream.write_all(&response)?;
                        connection.stream.flush()?;
                    }

                    connection.close();
                }
                _ => bail!(
                    "unknown packet_id {} during {:?}",
                    packet_id,
                    connection.state
                ),
            }

            return Ok(());
        }
        ConnectionState::Login => {
            let mut bytes = bytes;

            let packet_id_byte: u8 = binary::slice_reader::read_varint(&mut bytes)?.try_into()?;

            if let Ok(packet_id) = packet::login::client::PacketId::try_from(packet_id_byte) {
                println!("got packet by id: {:?}", packet_id);

                match packet_id {
                    packet::login::client::PacketId::LoginStart => {
                        let login_start_packet =
                            packet::login::client::LoginStart::read(&mut bytes)?;
                        slice_reader::ensure_fully_read(bytes)?;

                        println!("logging in with username: {}", login_start_packet.username);

                        let login_success_packet = packet::login::server::LoginSuccess {
                            uuid: rand::thread_rng().gen(),
                            username: login_start_packet.username,
                            property_count: 0
                        };

                        net::packet_helper::send_packet(
                            &mut connection.stream,
                            &login_success_packet,
                        )?;

                        connection.state = ConnectionState::Play;

                        // fake play, for testing

                        std::thread::sleep(std::time::Duration::from_millis(100));

                        let registry_codec = quartz_nbt::snbt::parse(include_str!("../assets/registry_codec.json"))?;
                        let mut binary: Vec<u8> = Vec::new();
                        quartz_nbt::io::write_nbt(&mut binary, None, &registry_codec,
                            quartz_nbt::io::Flavor::Uncompressed)?;
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
                            enable_respawn_screen: true,
                            is_debug: false,
                            is_flat: false,
                            has_death_location: false
                        };
                        net::packet_helper::send_packet(&mut connection.stream, &join_game_packet)?;

                        // Brand
                        let brand_packet = PluginMessage {
                            channel: "minecraft:brand",
                            data: b"\x08Graphite"
                        };
                        net::packet_helper::send_packet(&mut connection.stream, &brand_packet)?;

                        let mut heightmap_nbt = quartz_nbt::NbtCompound::new();
                        let mut motion_blocking_nbt = quartz_nbt::NbtList::new();
                        for i in 0..256 {
                            motion_blocking_nbt.push(0_i64);
                        }
                        heightmap_nbt.insert("MOTION_BLOCKING", motion_blocking_nbt);

                        let mut binary: Vec<u8> = Vec::new();
                        quartz_nbt::io::write_nbt(&mut binary, None, &heightmap_nbt,
                            quartz_nbt::io::Flavor::Uncompressed)?;
                        binary.shrink_to_fit();

                        
                        // Chunk
                        for x in -5..5 {
                            for z in -5..5 {
                                let mut chunk_data = vec!(0_u8; 0);
                                for i in 0..24 {
                                    chunk_data.put_i16(16*16*16); // block count

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
                                        trust_edges: false
                                    },
                                    chunk_light_data: ChunkLightData {
                                        sky_light_mask: vec![],
                                        block_light_mask: vec![],
                                        empty_sky_light_mask: vec![],
                                        empty_block_light_mask: vec![],
                                        sky_light_entries: vec![],
                                        block_light_entries: vec![],
                                    }
                                };
                                net::packet_helper::send_packet(&mut connection.stream, &chunk_packet)?;
                            }
                        }

                        // Update view position
                        let update_view_position_packet = UpdateViewPosition {
                            chunk_x: 0,
                            chunk_z: 0
                        };
                        net::packet_helper::send_packet(&mut connection.stream, &update_view_position_packet)?;

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
                        net::packet_helper::send_packet(&mut connection.stream, &position_packet)?;
                    }
                }
            } else {
                bail!(
                    "unknown packet_id {} during {:?}",
                    packet_id_byte,
                    connection.state
                );
            }

            return Ok(());
        }
        ConnectionState::Play => {
            let mut bytes = bytes;

            let packet_id_byte: u8 = binary::slice_reader::read_varint(&mut bytes)?.try_into()?;

            println!("got play packet: {:?}", packet_id_byte);

            return Ok(());
        }
        _ => {
            todo!()
        }
    }
}

fn send_serverlist_response(stream: &mut TcpStream) -> anyhow::Result<()> {
    const RESPONSE_JSON: &str = "{\
                \"version\": {
                    \"name\": \"1.19\",
                    \"protocol\": 759
                },
                \"players\": {
                    \"max\": 100,
                    \"online\": 5,
                    \"sample\": []
                },
                \"description\": {
                    \"text\": \"Hello world\"
                },
                \"favicon\": \"data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAEAAAABACAIAAAAlC+aJAAABGklEQVRo3u2aUQ7EIAhEbcNReiPP6Y16F/djk/1bozJASYffJu08BRxMj957yRxnSR4EIMDbAQTylrvWwdOrNTuAY6+NjhV7YiwDbEg3xVgDUKq3wIgp4rtW1FqYAEwuMAQDk0L/FE/q02TUqVR/tTb4vGkDBaTQjL4xIU/i91gJVNeDV8gZ+HnIorAGCJAAwKIBAACAhixyIvsyKL3Qg0bKqzXnbZlNoXmH/NwitvBkeuC1Ira2lk5daBvDAn6/iH9qAi+Fyva9EDDvlYTxVkJZx/RCBMgHgO1L3IEXAmANn+SV7r0DRk5b0im2BfAfaCRcn/JYkBIXwXejDzmPJZ1iVwCHAfrgD08EIAABCEAAAhCAAAQgwG58AEFWdXlZzlUbAAAAAElFTkSuQmCC\"
            }";

    let server_response = Response {
        json: RESPONSE_JSON,
    };
    net::packet_helper::send_packet(stream, &server_response)?;
    Ok(())
}
