use std::borrow::{Borrow, Cow};
use std::cell::{UnsafeCell, RefCell};
use std::io::Write;
use std::net::{ToSocketAddrs, SocketAddr, Ipv4Addr, IpAddr};
use std::pin::Pin;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use anyhow::bail;
use graphite_binary::slice_serialization::{Single, SliceSerializable};
use graphite_mc_protocol::login::serverbound::Hello;
use graphite_mc_protocol::status::serverbound::PingRequest;
use graphite_mc_protocol::status::clientbound::{StatusResponse, PongResponse};
use graphite_mc_protocol::types::GameProfile;
use graphite_mc_protocol::{handshake, status, login};
use graphite_mc_protocol::handshake::serverbound::Intention;
use graphite_network::{NetworkHandlerService, Connection, FramedPacketHandler, PacketBuffer, HandleAction};
use mio::net::TcpStream;
use slab::Slab;

enum Phase {
    Initial,
    Status,
    Login,
    LoginWaitForAck
}

struct ClientState {
    connection: Rc<RefCell<Connection>>,
    packet_buffer: PacketBuffer,
    concierge: *mut Concierge,
    phase: Phase,

    protocol_version: i32,
    connected_host: String,
    connected_port: u16,
    username: String,
    uuid: u128,

    received_status: bool,

    idx: Option<usize>
}

// todo: only keep connections for 10 seconds

impl FramedPacketHandler for ClientState {
    fn handle(&mut self, data: &[u8]) -> HandleAction {
        println!("Client state: received {}", String::from_utf8_lossy(data));

        let result = match self.phase {
            Phase::Initial => {
                handle_intention(self, data)
            },
            Phase::Status => {
                handle_status(self, data)
            },
            Phase::Login => {
                handle_login(self, data)
            },
            Phase::LoginWaitForAck => {
                handle_login_wait_for_ack(self, data)
            },
        };
        match result {
            Ok(action) => action,
            Err(error) => if cfg!(debug_assertions) {
                panic!("Encountered error in Concierge: {}", error);
            } else {
                HandleAction::Disconnect
            },
        }
    }

    fn disconnected(&mut self) {
        println!("Client state: disconnected");
        unsafe { self.concierge.as_mut() }.unwrap().client_states.remove(self.idx.unwrap());
    }
}

pub struct LoginInformation {
    pub username: String,
    pub uuid: u128
}

impl From<SocketAddr> for LoginInformation {
    fn from(_: SocketAddr) -> Self {
        panic!("unable to convert SocketAddr to LoginInformation")
    }
}

struct Concierge {
    client_states: Slab<Rc<UnsafeCell<ClientState>>>,
    sender: Box<dyn FnMut(LoginInformation, TcpStream)>,
    status: Arc<Mutex<String>>
}

impl NetworkHandlerService for Pin<Box<Concierge>> {
    const MAXIMUM_PACKET_SIZE: usize = 2097151;
    type ExtraData = SocketAddr;

    fn accept_new_connection(&mut self, _address: SocketAddr, connection: Rc<RefCell<Connection>>) {
        let state = Rc::new(UnsafeCell::new(ClientState {
            connection: connection.clone(),
            packet_buffer: PacketBuffer::new(),
            concierge: self.as_mut().get_mut(),
            phase: Phase::Initial,
            protocol_version: 0,
            connected_host: String::new(),
            connected_port: 0,
            username: String::new(),
            uuid: 0,
            received_status: false,
            idx: None
        }));

        let idx = self.client_states.insert(state.clone());
        unsafe { state.get().as_mut() }.unwrap().idx = Some(idx);

        connection.borrow_mut().set_handler(state.clone());

        println!("Got new connection, total: {}", self.client_states.len());
    }
}

pub fn listen(addr: impl ToSocketAddrs, sender: Box<dyn FnMut(LoginInformation, TcpStream)>, status: Arc<Mutex<String>>) {
    let mut handler = graphite_network::NetworkHandler::new(
        Box::pin(Concierge {
            client_states: Slab::new(),
            sender,
            status
        }),
        addr
    ).unwrap();
    handler.listen().unwrap();
}

fn handle_intention(client_state: &mut ClientState, mut bytes: &[u8]) -> anyhow::Result<HandleAction> {
    if bytes.len() < 3 {
        bail!("Insufficient bytes for handshake");
    } else if bytes[0..3] == [0xFE, 0x01, 0xFA] {
        bail!("Legacy server list ping is not supported");
    } else {
        let packet_id: u8 = Single::read(&mut bytes)?;
        if let Ok(packet_id) = handshake::serverbound::PacketId::try_from(packet_id) {
            match packet_id {
                handshake::serverbound::PacketId::Intention => {
                    let intention_packet = Intention::read_fully(&mut bytes)?;
                    println!("Read intention packet: {:?}", intention_packet);

                    match intention_packet.intention {
                        handshake::serverbound::IntentionType::Status => {
                            client_state.connected_host = intention_packet.host_name.to_string();
                            client_state.connected_port = intention_packet.port;
                            client_state.protocol_version = intention_packet.protocol_version;
                            client_state.phase = Phase::Status;
                        },
                        handshake::serverbound::IntentionType::Login => {
                            client_state.phase = Phase::Login;
                        },
                    }
                }
            }
        } else {
            bail!(
                "Unknown packet_id {} during intention",
                packet_id
            );
        }
    }

    Ok(HandleAction::Continue)
}

fn handle_status(client_state: &mut ClientState, mut bytes: &[u8]) -> anyhow::Result<HandleAction> {
    let packet_id: u8 = Single::read(&mut bytes)?;
    if let Ok(packet_id) = status::serverbound::PacketId::try_from(packet_id) {
        match packet_id {
            status::serverbound::PacketId::StatusRequest => {
                if client_state.received_status {
                    return Ok(HandleAction::Disconnect);
                }
                client_state.received_status = true;

                let status_str =  unsafe { client_state.concierge.as_mut() }.unwrap().status.lock().unwrap();

                let status_response = StatusResponse {
                    json: status_str.as_str(),
                };

                client_state.packet_buffer.write_packet(&status_response)?;

                drop(status_str);

                client_state.connection.borrow_mut().send(client_state.packet_buffer.pop_written());
            }
            status::serverbound::PacketId::PingRequest => {
                let ping_request = PingRequest::read_fully(&mut bytes)?;
                let pong_response = PongResponse {
                    time: ping_request.time
                };

                client_state.packet_buffer.write_packet(&pong_response)?;

                client_state.connection.borrow_mut().send(client_state.packet_buffer.pop_written());

                return Ok(HandleAction::Disconnect);
            }
        }
    } else {
        bail!(
            "Unknown packet_id {} during status",
            packet_id
        );
    }

    Ok(HandleAction::Continue)
}

fn handle_login(client_state: &mut ClientState, mut bytes: &[u8]) -> anyhow::Result<HandleAction> {
    let packet_id: u8 = Single::read(&mut bytes)?;
    if let Ok(packet_id) = login::serverbound::PacketId::try_from(packet_id) {
        match packet_id {
            login::serverbound::PacketId::Hello => {
                let hello = Hello::read_fully(&mut bytes)?;

                // Send login success
                let login_success = login::clientbound::LoginSuccess {
                    profile: GameProfile {
                        uuid: hello.uuid,
                        username: Cow::Borrowed(hello.username.borrow()),
                        properties: vec![],
                    },
                };

                client_state.packet_buffer.write_packet(&login_success)?;

                client_state.connection.borrow_mut().send(client_state.packet_buffer.pop_written());

                // Save information
                client_state.uuid = hello.uuid;
                client_state.username = hello.username.into_owned();
                client_state.phase = Phase::LoginWaitForAck;

                Ok(HandleAction::Continue)
            },
            login::serverbound::PacketId::LoginAcknowledged => {
                Ok(HandleAction::Disconnect)
            }
        }
    } else {
        bail!(
            "Unknown packet_id {} during login",
            packet_id
        );
    }
}

fn handle_login_wait_for_ack(client_state: &mut ClientState, mut bytes: &[u8]) -> anyhow::Result<HandleAction> {
    let packet_id: u8 = Single::read(&mut bytes)?;
    if let Ok(packet_id) = login::serverbound::PacketId::try_from(packet_id) {
        match packet_id {
            login::serverbound::PacketId::Hello => {
                Ok(HandleAction::Disconnect)
            },
            login::serverbound::PacketId::LoginAcknowledged => {
                // Redirect connection
                let concierge = unsafe { client_state.concierge.as_mut() }.unwrap();
                let login_information = LoginInformation {
                    username: std::mem::take(&mut client_state.username),
                    uuid: client_state.uuid
                };

                Ok(HandleAction::Transfer(Box::new(move |stream: TcpStream| {
                    (concierge.sender)(login_information, stream);
                })))
            }
        }
    } else {
        bail!(
            "Unknown packet_id {} during login",
            packet_id
        );
    }
}