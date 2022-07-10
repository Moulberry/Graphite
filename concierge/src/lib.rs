use std::{marker::PhantomData, time::Duration};

use anyhow::bail;
use binary::slice_serialization;
use binary::slice_serialization::SliceSerializable;
use bytes::BufMut;
use net::{
    network_buffer::WriteBuffer,
    network_handler::{
        Connection, ConnectionService, NetworkManagerService, NewConnectionAccepter,
        UninitializedConnection,
    },
    packet_helper::PacketReadResult,
};
use protocol::{handshake::client::Handshake, status::server::Response};
use rand::Rng;
use slab::Slab;

pub struct ProtoPlayer<T> {
    _phantom: PhantomData<T>,
    connection_state: ConnectionState,
    pub username: Option<String>,
    pub uuid: Option<u128>,
}

#[derive(Debug)]
pub enum ConnectionState {
    Handshake,
    Status,
    Login,
}

impl<T: ConciergeService + 'static> ConnectionService for ProtoPlayer<T> {
    type NetworkManagerServiceType = Concierge<T>;

    fn on_receive(
        &mut self,
        connection: &mut Connection<Self::NetworkManagerServiceType>,
    ) -> anyhow::Result<u32> {
        let mut bytes = connection.read_bytes();
        let mut write_buffer: WriteBuffer = WriteBuffer::new();

        let mut should_consume = false;

        loop {
            let packet_read_result = net::packet_helper::try_read_packet(&mut bytes)?;
            match packet_read_result {
                PacketReadResult::Complete(bytes) => {
                    println!("Request: {:x?}", bytes);
                    should_consume =
                        self.process_framed_packet(&mut write_buffer, connection, bytes)?;
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

        if should_consume {
            connection.request_redirect(
                |service: &mut Concierge<T>, connection, connection_service| {
                    service
                        .service
                        .accept_player(connection, connection_service);
                },
            );
        }

        Ok(bytes_remaining)
    }
}

impl<T: ConciergeService + 'static> ProtoPlayer<T> {
    fn process_framed_packet(
        &mut self,
        write_buffer: &mut WriteBuffer,
        connection: &Connection<Concierge<T>>,
        bytes: &[u8],
    ) -> anyhow::Result<bool> {
        match self.connection_state {
            ConnectionState::Handshake => {
                if bytes.len() < 3 {
                    bail!("insufficient bytes for handshake");
                } else if bytes[0..3] == [0xFE, 0x01, 0xFA] {
                    bail!("legacy server list ping from 2013 is not supported");
                } else {
                    // Handshake: https://wiki.vg/Protocol#Handshake
                    let mut bytes = bytes;

                    let packet_id_byte: u8 =
                        slice_serialization::VarInt::read(&mut bytes)?.try_into()?;

                    if let Ok(packet_id) =
                        protocol::handshake::client::PacketId::try_from(packet_id_byte)
                    {
                        println!("got packet by id: {:?}", packet_id);

                        let handshake_packet = Handshake::read(&mut bytes)?;
                        slice_serialization::check_empty(bytes)?;

                        self.connection_state = match handshake_packet.next_state {
                            1 => ConnectionState::Status,
                            2 => ConnectionState::Login,
                            next => bail!("unknown next state {} for ClientHandshake", next),
                        };
                    } else {
                        bail!(
                            "unknown packet_id {} during {:?}",
                            packet_id_byte,
                            self.connection_state
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
                        let concierge = &connection.get_network_manager().service;
                        let server_response = Response {
                            json: &concierge.serverlist_response,
                        };
                        net::packet_helper::write_packet(write_buffer, &server_response)?;
                    }
                    1 => {
                        if bytes.len() == 8 {
                            // todo: make this an actual packet
                            // length = 9, packet = 1, rest is copied over from `bytes`
                            let mut response: [u8; 10] = [9, 1, 0, 0, 0, 0, 0, 0, 0, 0];
                            response[2..].clone_from_slice(bytes);

                            write_buffer.get_unwritten(10).put_slice(&response);
                            unsafe { write_buffer.advance(10) };
                        }

                        // protoplayer.close();
                    }
                    _ => bail!(
                        "unknown packet_id {} during {:?}",
                        packet_id,
                        self.connection_state
                    ),
                }
            }
            ConnectionState::Login => {
                let mut bytes = bytes;

                let packet_id_byte: u8 =
                    slice_serialization::VarInt::read(&mut bytes)?.try_into()?;

                if let Ok(packet_id) = protocol::login::client::PacketId::try_from(packet_id_byte) {
                    println!("login - got packet by id: {:?}", packet_id);

                    match packet_id {
                        protocol::login::client::PacketId::LoginStart => {
                            let login_start_packet =
                                protocol::login::client::LoginStart::read(&mut bytes)?;
                            slice_serialization::check_empty(bytes)?;

                            println!("logging in with username: {}", login_start_packet.username);

                            // std::thread::sleep(std::time::Duration::from_millis(100));

                            self.username = Some(String::from(login_start_packet.username));
                            self.uuid = Some(rand::thread_rng().gen());

                            let login_success_packet = protocol::login::server::LoginSuccess {
                                uuid: self.uuid.unwrap(),
                                username: login_start_packet.username,
                                property_count: 0,
                            };

                            net::packet_helper::write_packet(write_buffer, &login_success_packet)?;

                            return Ok(true); // Consume the connection
                        }
                    }
                } else {
                    bail!(
                        "unknown packet_id {} during {:?}",
                        packet_id_byte,
                        self.connection_state
                    );
                }
            } /*net::ConnectionState::Play => {
                  let mut bytes = bytes;

                  let packet_id_byte: u8 = slice_serialization::VarInt::read(&mut bytes)?.try_into()?;

                  println!("got play packet: {:?}", packet_id_byte);
              }*/
        }

        Ok(false)
    }
}

pub struct Concierge<T: ConciergeService> {
    serverlist_response: String,
    service: T,
}

impl<'a, T: ConciergeService + 'static> NetworkManagerService for Concierge<T> {
    const TICK_RATE: Option<Duration> = Some(Duration::from_secs(10));
    type ConnectionServiceType = ProtoPlayer<T>;

    fn new_connection_service(&mut self) -> ProtoPlayer<T> {
        ProtoPlayer {
            _phantom: PhantomData,
            username: None,
            uuid: None,
            connection_state: ConnectionState::Handshake,
        }
    }

    /*fn consume_connection(&mut self, connection: Connection<Self>) {
        self.service.accept_player(connection);
    }*/

    fn tick(
        &mut self,
        _: &mut Slab<(Connection<Self>, Box<Self::ConnectionServiceType>)>,
        _: NewConnectionAccepter<Self>,
    ) -> anyhow::Result<()> {
        self.serverlist_response = self.service.get_serverlist_response();
        Ok(())
    }
}

impl<'a, T: ConciergeService + 'static> Concierge<T> {
    pub fn bind(addr: &str, mut service: T) -> anyhow::Result<()> {
        let concierge = Concierge {
            serverlist_response: service.get_serverlist_response(),
            service,
        };

        net::network_handler::start(concierge, Some(addr))?;

        Ok(())
    }
}

pub trait ConciergeService
where
    Self: Sized + 'static,
{
    fn get_serverlist_response(&mut self) -> String;
    fn accept_player(
        &mut self,
        player_connection: UninitializedConnection,
        player_service: &ProtoPlayer<Self>,
    );
}
