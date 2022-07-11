use std::{marker::PhantomData, time::Duration};

use anyhow::bail;
use binary::slice_serialization;
use binary::slice_serialization::SliceSerializable;
use net::{
    network_buffer::WriteBuffer,
    network_handler::{
        Connection, ConnectionService, NetworkManagerService, NewConnectionAccepter,
        UninitializedConnection, ConnectionSlab,
    },
    packet_helper::PacketReadResult,
};
use protocol::{
    handshake::{self, client::Intention},
    login,
    status::{self, client::PingRequest, server::StatusResponse},
};
use rand::Rng;
use slab::Slab;

pub struct ConciergeConnection<T> {
    _phantom: PhantomData<T>,
    connection_state: ConnectionState,
    pub username: Option<String>,
    pub uuid: Option<u128>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ConnectionState {
    Handshake,
    Status,
    Login,
}

impl<T: ConciergeService + 'static> ConnectionService for ConciergeConnection<T> {
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
                        self.handle_framed_packet(connection, &mut write_buffer, bytes)?;
                }
                PacketReadResult::Partial => break,
                PacketReadResult::Empty => break,
            }
        }

        let remaining_bytes = bytes.len() as u32;

        let to_write = write_buffer.get_written();
        if !to_write.is_empty() {
            connection.write(to_write);
        }

        if should_consume {
            if self.connection_state == ConnectionState::Login {
                connection.request_redirect(
                    |service: &mut Concierge<T>, connection, connection_service| {
                        service
                            .service
                            .accept_player(connection, connection_service);
                    },
                );
            } else {
                println!("requesting close");
                connection.request_close();
            }
        }

        Ok(remaining_bytes)
    }
}

impl<T: ConciergeService + 'static> ConciergeConnection<T> {
    fn handle_framed_packet(
        &mut self,
        connection: &Connection<Concierge<T>>,
        write_buffer: &mut WriteBuffer,
        mut bytes: &[u8],
    ) -> anyhow::Result<bool> {
        match self.connection_state {
            ConnectionState::Handshake => {
                // Handshake: https://wiki.vg/Handshake
                self.handle_handshake(&mut bytes)
            }
            ConnectionState::Status => {
                // Server List Ping: https://wiki.vg/Server_List_Ping
                self.handle_status(&mut bytes, connection, write_buffer)
            }
            ConnectionState::Login => {
                // todo: add reference to login protocol
                self.handle_login(&mut bytes, write_buffer)
            }
        }
    }

    fn handle_handshake(&mut self, bytes: &mut &[u8]) -> anyhow::Result<bool> {
        if bytes.len() < 3 {
            bail!("insufficient bytes for handshake");
        } else if bytes[0..3] == [0xFE, 0x01, 0xFA] {
            bail!("legacy server list ping from 2013 is not supported");
        } else {
            let packet_id: u8 = slice_serialization::VarInt::read(bytes)?.try_into()?;
            if let Ok(packet_id) = handshake::client::PacketId::try_from(packet_id) {
                match packet_id {
                    handshake::client::PacketId::Intention => {
                        let intention_packet = Intention::read_fully(bytes)?;

                        self.connection_state = match intention_packet.intention {
                            1 => ConnectionState::Status,
                            2 => ConnectionState::Login,
                            next => bail!("unknown intention ({}) during initial handshake", next),
                        };
                    }
                }
            } else {
                bail!(
                    "unknown packet_id {} during {:?}",
                    packet_id,
                    self.connection_state
                );
            }
        }

        Ok(false)
    }

    fn handle_status(
        &self,
        bytes: &mut &[u8],
        connection: &Connection<Concierge<T>>,
        write_buffer: &mut WriteBuffer,
    ) -> anyhow::Result<bool> {
        let packet_id: u8 = slice_serialization::VarInt::read(bytes)?.try_into()?;
        if let Ok(packet_id) = status::client::PacketId::try_from(packet_id) {
            match packet_id {
                status::client::PacketId::StatusRequest => {
                    let concierge = &connection.get_network_manager().service;
                    let server_response = StatusResponse {
                        json: &concierge.serverlist_response,
                    };
                    net::packet_helper::write_packet(write_buffer, &server_response)?;
                }
                status::client::PacketId::PingRequest => {
                    let ping_request = PingRequest::read_fully(bytes)?;
                    net::packet_helper::write_packet(write_buffer, &ping_request)?;

                    // todo: flush write & then close connection, bad actors could create a never ending connection
                }
            }
        } else {
            bail!(
                "unknown packet_id {} during {:?}",
                packet_id,
                self.connection_state
            );
        }

        Ok(false)
    }

    fn handle_login(
        &mut self,
        bytes: &mut &[u8],
        write_buffer: &mut WriteBuffer,
    ) -> anyhow::Result<bool> {
        let packet_id: u8 = slice_serialization::VarInt::read(bytes)?.try_into()?;
        if let Ok(packet_id) = login::client::PacketId::try_from(packet_id) {
            match packet_id {
                login::client::PacketId::Hello => {
                    let login_start_packet = login::client::Hello::read_fully(bytes)?;

                    self.username = Some(String::from(login_start_packet.username));
                    self.uuid = Some(rand::thread_rng().gen());

                    let login_success_packet = login::server::LoginSuccess {
                        uuid: self.uuid.unwrap(),
                        username: login_start_packet.username,
                        property_count: 0,
                    };

                    net::packet_helper::write_packet(write_buffer, &login_success_packet)?;

                    Ok(true) // Consume the connection
                }
            }
        } else {
            bail!(
                "unknown packet_id {} during {:?}",
                packet_id,
                self.connection_state
            );
        }
    }
}

pub struct Concierge<T: ConciergeService> {
    serverlist_response: String,
    service: T,
}

impl<T: ConciergeService + 'static> NetworkManagerService for Concierge<T> {
    const TICK_RATE: Option<Duration> = Some(Duration::from_secs(10));
    type ConnectionServiceType = ConciergeConnection<T>;

    fn new_connection_service(&mut self) -> ConciergeConnection<T> {
        ConciergeConnection {
            _phantom: PhantomData,
            username: None,
            uuid: None,
            connection_state: ConnectionState::Handshake,
        }
    }

    fn tick(
        &mut self,
        _: &mut ConnectionSlab<Self>,
        _: NewConnectionAccepter<Self>,
    ) -> anyhow::Result<()> {
        self.serverlist_response = self.service.get_serverlist_response();
        Ok(())
    }
}

impl<T: ConciergeService + 'static> Concierge<T> {
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
        player_service: &ConciergeConnection<Self>,
    );
}
