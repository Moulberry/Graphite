use std::{marker::PhantomData, time::Duration};

use anyhow::{bail, anyhow};
use binary::slice_serialization;
use binary::slice_serialization::SliceSerializable;
use net::{
    network_buffer::WriteBuffer,
    network_handler::{
        Connection, ConnectionService, ConnectionSlab, NetworkManagerService,
        NewConnectionAccepter, UninitializedConnection,
    },
    packet_helper::PacketReadResult,
};
use protocol::{
    handshake::{self, client::Intention},
    login,
    status::{self, client::PingRequest, server::StatusResponse}, types::{GameProfile, GameProfileProperty},
};
use rand::Rng;

pub struct ConciergeConnection<T> {
    _phantom: PhantomData<T>,
    connection_state: ConnectionState,
    pub game_profile: Option<GameProfile>
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
                    // println!("Request: {:x?}", bytes);
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
                // Login: https://wiki.vg/Protocol#Login
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
                    
                    let uuid = rand::thread_rng().gen();//login_start_packet.uuid.ok_or(anyhow!("invalid uuid"))?;

                    println!("player joined with uuid: {:x}", uuid);

                    // todo: download skin
                    let game_profile = GameProfile {
                        uuid,
                        username: login_start_packet.username.into(),
                        properties: vec![
                            GameProfileProperty {
                                id: "textures".into(),
                                value: "ewogICJ0aW1lc3RhbXAiIDogMTY1OTAyMDI4NjQ0OCwKICAicHJvZmlsZUlkIiA6ICJkMGUwNWRlNzYwNjc0NTRkYmVhZWM2ZDE5ZDg4NjE5MSIsCiAgInByb2ZpbGVOYW1lIiA6ICJNb3VsYmVycnkiLAogICJzaWduYXR1cmVSZXF1aXJlZCIgOiB0cnVlLAogICJ0ZXh0dXJlcyIgOiB7CiAgICAiU0tJTiIgOiB7CiAgICAgICJ1cmwiIDogImh0dHA6Ly90ZXh0dXJlcy5taW5lY3JhZnQubmV0L3RleHR1cmUvYmNlMTU1MjI0ZWE0YmM0OWE4ZTkxOTA3MzdjYjA0MTdkOGE3YzM4YTAzN2Q4ZDAzODJkZGU0ODI5YzEwMzU5MCIsCiAgICAgICJtZXRhZGF0YSIgOiB7CiAgICAgICAgIm1vZGVsIiA6ICJzbGltIgogICAgICB9CiAgICB9CiAgfQp9".into(),
                                signature: Some("XMgxJ45DZlaKr3BozEJ9tYUpqqhN/WIvHt8T8KGnbYjUFGq5q3WOodpR/2hlBE5dgTL+wk3QFXXuBYzDmcKVPl3Nh/Qv3ZqETOZQ1hC5hLTpNwCKH55QGRqEQYwLEZ+4fz2bdTqd+nISehl6fEwHLb3mSXIj7n/ICxJ0jPw9+1BDndY2omKRjnD8G3VRf3gAhcwMw5mCTy3RMOa+3VIe4YTUqQFSOqQ7H1JTmD1mzXbGaqJaOg6DOFlI+nXXNuajfqr2TEiK78ieZk78mzvYB5K/5NH2NttKmuDYNVyR9u5f9IyRpEFba0tIEC1DpfbSu7TgNb5tIXTxzr9W0sG+OyVN2+/hO1vxejvYpSFJki/O1E5UHLKAilVr4IVjnpMNsY/6TS6C83UTz3UGXghSuSiX77xMGikzgJmUaNFjUoCe1jzdu3aBA/PPCXVQh17CBilVWFFUE5qapKphp9rPD2KpaOjPyRv9dWEx1c0VFhAUWDcoM4/6dnqdpR8AGzZSBLNpAL+DfaZ83qwfZ8GIqvDdYbvz09A9DHEOhgy3qoPvgwCMKTdsTsrQhVOVxKo0s0hNDiDu3ZKpF3SA2OXcaRES+B/xWSQ9Lcq1G9++v+0TWiKS+3oyecUCIQcdrQZQDxKXgVPUUo1XXUEgCjEdCUy0OuWmSQCrSBhWG6bfguk=".into())
                            }
                        ]
                    };

                    // Write login success
                    let login_success_packet = login::server::LoginSuccess {
                        profile: game_profile
                    };
                    net::packet_helper::write_packet(write_buffer, &login_success_packet)?;

                    // Set game profile
                    self.game_profile = Some(login_success_packet.profile);
                    
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
            game_profile: None,
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
        player_service: ConciergeConnection<Self>,
    );
}
