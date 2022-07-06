use std::io::prelude::*;
use std::net::TcpStream;

use anyhow::bail;
use bytes::BufMut;

use binary::slice_reader;
use concierge::Concierge;
use concierge::ConciergeService;
use net::network_buffer::{PacketReadBuffer, PacketReadResult};
use protocol::handshake::client::Handshake;
use protocol::play::server::ChunkBlockData;
use protocol::play::server::ChunkDataAndUpdateLight;
use protocol::play::server::ChunkLightData;
use protocol::play::server::JoinGame;
use protocol::play::server::PlayerPositionAndLook;
use protocol::play::server::PluginMessage;
use protocol::play::server::UpdateViewPosition;
use protocol::status::server::Response;
use rand::Rng;

struct MyConciergeImpl {
    counter: u8
}

impl ConciergeService for MyConciergeImpl {
    fn get_message(&mut self) -> String {
        self.counter += 1;
        let string = String::from(format!("times called: {}", self.counter));
        string
    }
}

fn main() {
    Concierge::bind("127.0.0.1:25565", MyConciergeImpl {
        counter: 0
    }).unwrap();

    /*let listener = TcpListener::bind("127.0.0.1:25565").unwrap();

    //let map: HashMap<UUID, Player> = HashMap::new();

    for stream in listener.incoming() {
        let stream = stream.unwrap();

        let connection = net::PlayerConnection {
            stream,
            state: net::ConnectionState::Handshake,
            closed: false,
        };

        handle_connection(connection);
    }
    */
}

/*fn handle_connection(mut connection: net::PlayerConnection) {
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
}*/

use binary::slice_serializable::SliceSerializable;


