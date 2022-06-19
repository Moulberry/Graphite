use std::io::prelude::*;
use std::net::TcpListener;
use std::net::TcpStream;

mod binary_buffer;
mod network_buffer;
mod varint;

use anyhow::bail;
use bytes::Buf;
use network_buffer::{PacketReadBuffer, PacketReadResult};

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
                let mut binary_buf = binary_buffer::BinaryBuf::new(bytes);

                let packet_id = binary_buf.get_varint()?;
                if packet_id != 0 {
                    bail!("unknown packet_id {} during {:?}", packet_id, connection.state);
                }

                let _protocol_version = binary_buf.get_varint()?;
                let _connected_from = binary_buf.get_string_with_max_size(256)?;
                let _port = binary_buf.get_u16();
                let next_state = binary_buf.get_varint()?;

                binary_buf.check_finished()?;

                connection.state = match next_state {
                    1 => ConnectionState::Status,
                    2 => ConnectionState::Login,
                    next => bail!("unknown next state {} during {:?}", next, connection.state)
                };

                //send_serverlist_response(&mut connection.stream);

                return Ok(());
            }
        }
        ConnectionState::Status => {
            // Server List Ping: https://wiki.vg/Server_List_Ping
            let mut binary_buf = binary_buffer::BinaryBuf::new(bytes);

            let packet_id = binary_buf.get_varint()?;
            match packet_id {
                0 => send_serverlist_response(&mut connection.stream)?,
                1 => {
                    connection.stream.write_all(&[9, 1])?;
                    connection.stream.write_all(binary_buf.get_all_bytes()?)?;
                    connection.stream.flush()?;
                    connection.close();
                },
                _ => bail!("unknown packet_id {} during {:?}", packet_id, connection.state)
            }

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
    let mut bytes: Vec<u8> = vec![];
    push_varint(&mut bytes, 1 + 2 + RESPONSE_JSON.len() as i32);
    bytes.push(0);
    push_varint(&mut bytes, RESPONSE_JSON.len() as i32);
    bytes.extend_from_slice(RESPONSE_JSON.as_bytes());
    stream.write_all(&bytes)?;
    Ok(())
}

// todo: move encoding to varint.rs

fn push_varint(vec: &mut Vec<u8>, num: i32) {
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
}