use std::io::prelude::*;
use std::net::TcpListener;
use std::net::TcpStream;

mod binary;
mod net;
mod packet;

use anyhow::bail;

use net::network_buffer::{PacketReadBuffer, PacketReadResult};
use packet::Packet;
use rand::Rng;
use crate::packet::handshake::ClientHandshake;
use crate::packet::login::ClientLoginStart;
use crate::packet::login::ServerLoginSuccess;
use crate::packet::play::ServerJoinGame;
use crate::packet::status::ServerResponse;

#[derive(PartialEq, Eq)]
struct Uuid(u128);

#[derive(Debug)]
enum ConnectionState {
    Handshake,
    Status,
    Login,
    Play
}

struct PlayerConnection {
    stream: TcpStream,
    state: ConnectionState,
    closed: bool
}

fn main() {
    let listener = TcpListener::bind("127.0.0.1:25565").unwrap();

    //let map: HashMap<UUID, Player> = HashMap::new();

    for stream in listener.incoming() {
        let stream = stream.unwrap();

        let connection = PlayerConnection{stream, state: ConnectionState::Handshake, closed: false};

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
                    PacketReadResult::Empty => break
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

                let packet_id_byte: u8 = binary::slice_reader::read_varint(&mut bytes)?.try_into()?;

                if let Ok(packet_id) = packet::handshake::ClientPacketId::try_from(packet_id_byte) {
                    println!("got packet by id: {:?}", packet_id);

                    let handshake_packet = ClientHandshake::read(&mut bytes)?;

                    connection.state = match handshake_packet.next_state {
                        1 => ConnectionState::Status,
                        2 => ConnectionState::Login,
                        next => bail!("unknown next state {} for ClientHandshake", next)
                    };
                } else {
                    bail!("unknown packet_id {} during {:?}", packet_id_byte, connection.state);
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
                },
                _ => bail!("unknown packet_id {} during {:?}", packet_id, connection.state)
            }

            return Ok(());
        },
        ConnectionState::Login => {
            let mut bytes = bytes;

            let packet_id_byte: u8 = binary::slice_reader::read_varint(&mut bytes)?.try_into()?;

            if let Ok(packet_id) = packet::login::ClientPacketId::try_from(packet_id_byte) {
                println!("got packet by id: {:?}", packet_id);

                match packet_id {
                    packet::login::ClientPacketId::ClientLoginStart => {
                        let login_start_packet = ClientLoginStart::read(bytes)?;
                        println!("logging in with username: {}", login_start_packet.username);

                        let login_success_packet = ServerLoginSuccess {
                            uuid: rand::thread_rng().gen(),
                            username: login_start_packet.username
                        };

                        net::packet_helper::send_packet(&mut connection.stream, login_success_packet)?;

                        // connection.state = ConnectionState::Play;

                        // fake play, for testing

                        std::thread::sleep(std::time::Duration::from_secs(1));

                        let join_game_packet = ServerJoinGame {
                            entity_id: 0
                        };
                        net::packet_helper::send_packet(&mut connection.stream, join_game_packet)?;

                        std::thread::sleep(std::time::Duration::from_secs(1));

                        connection.close();
                    }
                }
            } else {
                bail!("unknown packet_id {} during {:?}", packet_id_byte, connection.state);
            }

            return Ok(());
        },
        _ => {
            todo!()
        }
    }
}

fn send_serverlist_response(stream: &mut TcpStream) -> anyhow::Result<()> {
    const RESPONSE_JSON: &str = "{\
                \"version\": {
                    \"name\": \"1.18.2\",
                    \"protocol\": 758
                },
                \"players\": {
                    \"max\": 100,
                    \"online\": 5,
                    \"sample\": [
                        {
                            \"name\": \"thinkofdeath\",
                            \"id\": \"4566e69f-c907-48ee-8d71-d7ba5aa00d20\"
                        }
                    ]
                },
                \"description\": {
                    \"text\": \"Hello world\"
                },
                \"favicon\": \"data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAEAAAABACAIAAAAlC+aJAAABGklEQVRo3u2aUQ7EIAhEbcNReiPP6Y16F/djk/1bozJASYffJu08BRxMj957yRxnSR4EIMDbAQTylrvWwdOrNTuAY6+NjhV7YiwDbEg3xVgDUKq3wIgp4rtW1FqYAEwuMAQDk0L/FE/q02TUqVR/tTb4vGkDBaTQjL4xIU/i91gJVNeDV8gZ+HnIorAGCJAAwKIBAACAhixyIvsyKL3Qg0bKqzXnbZlNoXmH/NwitvBkeuC1Ira2lk5daBvDAn6/iH9qAi+Fyva9EDDvlYTxVkJZx/RCBMgHgO1L3IEXAmANn+SV7r0DRk5b0im2BfAfaCRcn/JYkBIXwXejDzmPJZ1iVwCHAfrgD08EIAABCEAAAhCAAAQgwG58AEFWdXlZzlUbAAAAAElFTkSuQmCC\"
            }";

    let server_response = ServerResponse { json: RESPONSE_JSON };
    net::packet_helper::send_packet(stream, server_response)?;
    Ok(())
}