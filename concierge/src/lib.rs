use anyhow::bail;
use binary::{slice_serializable::SliceSerializable, slice_reader};
use bytes::BufMut;
use net::{network_handler::{Connection, BackloggedSubmissionQueue, NetworkManagerService, ConnectionService, ByteSender}, packet_helper::PacketReadResult, ConnectionState};
use protocol::{handshake::client::Handshake, status::server::Response, play::server::{JoinGame, PluginMessage, ChunkDataAndUpdateLight, ChunkBlockData, ChunkLightData, UpdateViewPosition, PlayerPositionAndLook}};
use rand::Rng;

struct ProtoPlayer {
    connection_state: ConnectionState,
}

impl <T: ConciergeService> ConnectionService<Concierge<T>> for ProtoPlayer {
    fn receive(&mut self, service: &mut Concierge<T>, mut bytes: &[u8], byte_sender: &mut impl ByteSender) {
        loop {
            if let Ok(packet_read_result) = net::packet_helper::try_read_packet(&mut bytes) {
                match packet_read_result {
                    PacketReadResult::Complete(bytes) => {
                        println!("Request: {:x?}", bytes);
                        if let Err(e) = process_framed_packet(self, byte_sender, bytes) {
                            println!("got error: {:?}", e);
                            // todo: maybe inform player of error via disconnect packet?
                            // connection.close();
                        }
                    }
                    PacketReadResult::Partial => {
                        todo!();
                    }
                    PacketReadResult::Empty => break,
                }
            } else {
                // todo: maybe inform player of error via disconnect packet?
                // connection.close();
            }
        }
    }
}

pub struct Concierge<T: ConciergeService + 'static> {
    service: T
}

impl <'a, T: ConciergeService + 'static> NetworkManagerService<ProtoPlayer> for Concierge<T> {
    const SHOULD_TICK: bool = false;

    fn new_connection_service(&self) -> ProtoPlayer {
        ProtoPlayer {
            connection_state: ConnectionState::Handshake
        }
    }
}

impl <'a, T: ConciergeService + 'static> Concierge<T> {
    pub fn bind(addr: &str, service: T) -> anyhow::Result<()> {
        let concierge = Concierge { service };
        net::network_handler::start(concierge, addr)?;

        Ok(())
    }
}

pub trait ConciergeService {
    fn get_message(&mut self) -> String;
    // fn handle_player_join()
    //fn get_serverlist_response() -> String;
}

fn process_framed_packet(protoplayer: &mut ProtoPlayer, byte_sender: &mut impl ByteSender, bytes: &[u8]) -> anyhow::Result<()> {
    match protoplayer.connection_state {
        net::ConnectionState::Handshake => {
            if bytes.len() < 3 {
                bail!("insufficient bytes for handshake");
            } else if bytes[0..3] == [0xFE, 0x01, 0xFA] {
                bail!("legacy server list ping from 2013 is not supported");
            } else {
                // Handshake: https://wiki.vg/Protocol#Handshake
                let mut bytes = bytes;

                let packet_id_byte: u8 =
                    binary::slice_reader::read_varint(&mut bytes)?.try_into()?;

                if let Ok(packet_id) =
                    protocol::handshake::client::PacketId::try_from(packet_id_byte)
                {
                    println!("got packet by id: {:?}", packet_id);

                    let handshake_packet = Handshake::read(&mut bytes)?;
                    slice_reader::ensure_fully_read(bytes)?;

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

            let packet_id = binary::slice_reader::read_varint(&mut bytes)?;
            match packet_id {
                0 => send_serverlist_response(byte_sender)?,
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

            let packet_id_byte: u8 = binary::slice_reader::read_varint(&mut bytes)?.try_into()?;

            if let Ok(packet_id) = protocol::login::client::PacketId::try_from(packet_id_byte) {
                println!("login - got packet by id: {:?}", packet_id);

                match packet_id {
                    protocol::login::client::PacketId::LoginStart => {
                        let login_start_packet =
                            protocol::login::client::LoginStart::read(&mut bytes)?;
                        slice_reader::ensure_fully_read(bytes)?;

                        println!("logging in with username: {}", login_start_packet.username);

                        std::thread::sleep(std::time::Duration::from_millis(100));

                        let login_success_packet = protocol::login::server::LoginSuccess {
                            uuid: rand::thread_rng().gen(),
                            username: login_start_packet.username,
                            property_count: 0,
                        };

                        println!("sending login success!");

                        net::packet_helper::send_packet(
                            byte_sender,
                            &login_success_packet,
                        )?;

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

            let packet_id_byte: u8 = binary::slice_reader::read_varint(&mut bytes)?.try_into()?;

            println!("got play packet: {:?}", packet_id_byte);
        }
    }

    Ok(())
}

fn send_serverlist_response(byte_sender: &mut impl ByteSender) -> anyhow::Result<()> {
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
    net::packet_helper::send_packet(byte_sender, &server_response)?;
    Ok(())
}