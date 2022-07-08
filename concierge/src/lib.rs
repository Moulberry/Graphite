use std::{time::Duration, collections::VecDeque};

use anyhow::bail;
use binary::slice_serialization;
use binary::slice_serialization::SliceSerializable;
use io_uring::{SubmissionQueue, squeue};
use net::{
    network_handler::{
        ByteSender, ConnectionService, NetworkManagerService, Connection,
    },
    packet_helper::PacketReadResult
};
use protocol::{
    handshake::client::Handshake,
    status::server::Response,
};
use rand::Rng;
use slab::Slab;

pub struct ProtoPlayer {
    connection_state: ConnectionState,
    pub username: Option<String>,
    pub uuid: Option<u128>
}

#[derive(Debug)]
pub enum ConnectionState {
    Handshake,
    Status,
    Login
}

impl<T: ConciergeService> ConnectionService<Concierge<T>> for ProtoPlayer {
    fn on_receive(
        &mut self,
        service: &mut Concierge<T>,
        bytes: &mut &[u8],
        mut byte_sender: ByteSender,
    ) -> anyhow::Result<bool> {
        loop {
            let packet_read_result = net::packet_helper::try_read_packet(bytes)?;
            match packet_read_result {
                PacketReadResult::Complete(bytes) => {
                    println!("Request: {:x?}", bytes);
                    let should_consume = service.process_framed_packet(self, &mut byte_sender, bytes)?;
                    if should_consume {
                        return Ok(true)
                    }
                }
                PacketReadResult::Partial => return Ok(false),
                PacketReadResult::Empty => return Ok(false),
            }
        }
    }

    fn on_created(&mut self, _: ByteSender) {
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
            username: None,
            uuid: None,
            connection_state: ConnectionState::Handshake,
        }
    }

    fn tick(&mut self, _: &mut Slab<Connection<ProtoPlayer>>, _: SubmissionQueue, _: &mut VecDeque<squeue::Entry>) {
        self.serverlist_response = self.service.get_serverlist_response();
    }

    fn consume_connection(&mut self, connection: Connection<ProtoPlayer>) {
        self.service.accept_player(connection);
    }
}

impl<'a, T: ConciergeService + 'static> Concierge<T> {
    pub fn bind(addr: &str, mut service: T) -> anyhow::Result<()> {
        let concierge = Concierge {
            serverlist_response: service.get_serverlist_response(),
            service
        };

        net::network_handler::start(concierge, Some(addr))?;

        Ok(())
    }

    fn process_framed_packet(
        &mut self,
        protoplayer: &mut ProtoPlayer,
        byte_sender: &mut ByteSender,
        bytes: &[u8],
    ) -> anyhow::Result<bool> {
        match protoplayer.connection_state {
            ConnectionState::Handshake => {
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
                            1 => ConnectionState::Status,
                            2 => ConnectionState::Login,
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
            ConnectionState::Status => {
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
    
                            byte_sender.send(&response);
                        }
    
                        // protoplayer.close();
                    }
                    _ => bail!(
                        "unknown packet_id {} during {:?}",
                        packet_id,
                        protoplayer.connection_state
                    ),
                }
            }
            ConnectionState::Login => {
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
    
                            // std::thread::sleep(std::time::Duration::from_millis(100));

                            protoplayer.username = Some(String::from(login_start_packet.username));
                            protoplayer.uuid = rand::thread_rng().gen();
    
                            let login_success_packet = protocol::login::server::LoginSuccess {
                                uuid: protoplayer.uuid.unwrap(),
                                username: login_start_packet.username,
                                property_count: 0,
                            };
    
                            net::packet_helper::send_packet(byte_sender, &login_success_packet)?;

                            return Ok(true); // Consume the connection
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
            /*net::ConnectionState::Play => {
                let mut bytes = bytes;
    
                let packet_id_byte: u8 = slice_serialization::VarInt::read(&mut bytes)?.try_into()?;
    
                println!("got play packet: {:?}", packet_id_byte);
            }*/
        }
    
        Ok(false)
    }
}

pub trait ConciergeService {
    fn get_serverlist_response(&mut self) -> String;
    fn accept_player(&mut self, player_connection: Connection<ProtoPlayer>);
}