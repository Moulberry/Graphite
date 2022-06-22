use std::io::prelude::*;
use std::net::TcpListener;
use std::net::TcpStream;

mod network_buffer;
mod varint;
mod packet;
mod binary_reader;
mod binary_writer;

use anyhow::bail;

use network_buffer::{PacketReadBuffer, PacketReadResult};
use packet::Packet;
use packet::IdentifiedPacket;
use rand::Rng;
use crate::packet::handshake::ClientHandshake;
use crate::packet::login::ClientLoginStart;
use crate::packet::login::ServerLoginSuccess;
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

                let packet_id_byte: u8 = binary_reader::read_varint(&mut bytes)?.try_into()?;

                if let Ok(packet_id) = packet::handshake::ClientPacketId::try_from(packet_id_byte) {
                    println!("got packet by id: {:?}", packet_id);

                    let handshake_packet = ClientHandshake::read(bytes)?;

                    connection.state = match handshake_packet.next_state {
                        1 => ConnectionState::Status,
                        2 => ConnectionState::Login,
                        next => bail!("unknown next state {} during {:?}", next, connection.state)
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

            let packet_id = binary_reader::read_varint(&mut bytes)?;
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

            let packet_id_byte: u8 = binary_reader::read_varint(&mut bytes)?.try_into()?;

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

                        send_packet(&mut connection.stream, login_success_packet)?;
                    }
                }

                /*let handshake_packet = ClientHandshake::read(bytes)?;

                connection.state = match handshake_packet.next_state {
                    1 => ConnectionState::Status,
                    2 => ConnectionState::Login,
                    next => bail!("unknown next state {} during {:?}", next, connection.state)
                };*/
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

fn send_packet<'a, I, T: Packet<'a, I>>(stream: &mut TcpStream, packet: T) -> anyhow::Result<()> {
    let expected_packet_size = packet.get_write_size();
    if expected_packet_size > 2097148 {
        bail!("packet too large!");
    }

    let mut bytes = vec![0; 4 + expected_packet_size];

    // invariant should be satisfied because we allocated at least `get_write_size` bytes
    let slice_after_writing = unsafe { packet.write(&mut bytes[4..]) };
    let bytes_written = expected_packet_size - slice_after_writing.len();

    // println!("wrote bytes: {}", bytes_written);

    let (varint_raw, written) = varint::encode::i32_raw(1 + bytes_written as i32);
    if written > 3 {
        bail!("packet too large!");
    }

    // println!("{:?}", varint_raw);

    let varint_bytes_spare = 3 - written;

    bytes[varint_bytes_spare..3].copy_from_slice(&varint_raw[..written]);
    bytes[3] = packet.get_packet_id_as_u8();

    // println!("bytes: {:?}", &bytes[varint_bytes_spare..4+bytes_written]);

    stream.write_all(&bytes[varint_bytes_spare..4+bytes_written])?;
    stream.flush()?;

    Ok(())
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
    send_packet(stream, server_response)?;
    Ok(())
}

// todo: move encoding to varint.rs

/*fn push_varint(vec: &mut Vec<u8>, num: i32) {
    let (bytes, size) = unsafe { encode_varint(num) };
    vec.extend_from_slice(&bytes[..size]);
}

unsafe fn encode_varint(num: i32) -> ([u8; 16], usize) {
    let x = std::mem::transmute::<i32, u32>(num) as u64; 
    let stage1 = (x & 0x000000000000007f)
        | ((x & 0x0000000000003f80) << 1)
        | ((x & 0x00000000001fc000) << 2)
        | ((x & 0x000000000fe00000) << 3)
        | ((x & 0x00000000f0000000) << 4);

    let leading = stage1.leading_zeros();

    let unused_bytes = (leading - 1) / 8;
    let bytes_needed = 8 - unused_bytes;

    // set all but the last MSBs
    let msbs = 0x8080808080808080;
    let msbmask = 0xFFFFFFFFFFFFFFFF >> ((8 - bytes_needed + 1) * 8 - 1);

    let merged = stage1 | (msbs & msbmask);

    (std::mem::transmute([merged, 0]), bytes_needed as usize)
}*/