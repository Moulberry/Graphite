use std::borrow::{Borrow, Cow};
use std::cell::{UnsafeCell, RefCell};

use std::net::{ToSocketAddrs, SocketAddr};
use std::pin::Pin;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::bail;
use graphite_binary::slice_serialization::*;
use graphite_mc_protocol::login::serverbound::Hello;
use graphite_mc_protocol::status::serverbound::PingRequest;
use graphite_mc_protocol::status::clientbound::{StatusResponse, PongResponse};
use graphite_mc_protocol::types::{GameProfile, GameProfileProperty};
use graphite_mc_protocol::{handshake, login, status, IdentifiedPacket};
use graphite_mc_protocol::handshake::serverbound::Intention;
use graphite_network::{Connection, FramedPacketHandler, HandleAction, NetworkHandlerService, PacketBuffer, SendableConnection, ServiceTickAction};
use hmac::{Hmac, Mac};
use rand::Rng;
use sha2::Sha256;
use slab::Slab;

enum Phase {
    Initial,
    Status,
    Login,
    LoginWaitForAck,
    LoginWaitForVelocityResponse,
}

struct ClientState {
    connection: Rc<RefCell<Connection>>,
    packet_buffer: PacketBuffer,
    concierge: *mut Concierge,
    authentication: AuthenticationMode,
    phase: Phase,

    protocol_version: i32,
    connected_host: String,
    connected_port: u16,
    profile: Option<GameProfile<'static>>,

    received_status: bool,
    query_transaction_id: i32,
    connected_seconds: u8,

    idx: usize
}

impl FramedPacketHandler for ClientState {
    fn handle(&mut self, data: &[u8]) -> HandleAction {
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
            Phase::LoginWaitForVelocityResponse => {
                handle_login_wait_for_velocity_response(self, data)
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
        unsafe { self.concierge.as_mut() }.unwrap().client_states.try_remove(self.idx);
    }
}

#[derive(Debug)]
pub struct LoginInformation {
    pub profile: GameProfile<'static>
}

impl TryFrom<SocketAddr> for LoginInformation {
    type Error = ();
    
    fn try_from(_: SocketAddr) -> Result<Self, Self::Error> {
        Err(())
    }
}

#[derive(Clone)]
pub enum AuthenticationMode {
    None,
    Velocity(Vec<u8>)
}

struct Concierge {
    client_states: Slab<Rc<UnsafeCell<ClientState>>>,
    sender: Box<dyn FnMut(LoginInformation, SendableConnection)>,
    status: Arc<Mutex<String>>,
    authentication: AuthenticationMode
}

impl NetworkHandlerService for Pin<Box<Concierge>> {
    const MAXIMUM_PACKET_SIZE: usize = 2097151;
    const TICK_RATE: Option<std::time::Duration> = Some(Duration::from_secs(1));
    type ExtraData = SocketAddr;

    fn accept_new_connection(&mut self, _address: SocketAddr, connection: Rc<RefCell<Connection>>) {
        let concierge = self.as_mut().get_mut() as *mut Concierge;
        let authentication = self.authentication.clone();

        let vacant = self.client_states.vacant_entry();

        let state = Rc::new(UnsafeCell::new(ClientState {
            connection: connection.clone(),
            packet_buffer: PacketBuffer::new(),
            concierge,
            authentication,
            phase: Phase::Initial,
            protocol_version: 0,
            connected_host: String::new(),
            connected_port: 0,
            profile: None,
            received_status: false,
            query_transaction_id: 0,
            connected_seconds: 0,
            idx: vacant.key()
        }));

        vacant.insert(state.clone());
        connection.borrow_mut().set_handler(state);
    }

    fn tick(&mut self) -> ServiceTickAction {
        self.client_states.retain(|_, state| {
            let state = unsafe { state.get().as_mut() }.unwrap();
            state.connected_seconds += 1;
            if state.connected_seconds > 10 {
                state.connection.borrow_mut().shutdown();
                false
            } else {
                true
            }
            
        });
        ServiceTickAction::None
    }
}

pub fn listen(addr: impl ToSocketAddrs, sender: Box<dyn FnMut(LoginInformation, SendableConnection)>, status: Arc<Mutex<String>>,
        authentication: AuthenticationMode) {
    let mut handler = graphite_network::NetworkHandler::new(
        Box::pin(Concierge {
            client_states: Slab::new(),
            sender,
            status,
            authentication
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

                    client_state.connected_host = intention_packet.host_name.to_string();
                    client_state.connected_port = intention_packet.port;
                    client_state.protocol_version = intention_packet.protocol_version;

                    match intention_packet.intention {
                        handshake::serverbound::IntentionType::Status => {
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

                status_response.write_packet(&mut client_state.packet_buffer);

                drop(status_str);

                client_state.connection.borrow_mut().send(&mut client_state.packet_buffer);
            }
            status::serverbound::PacketId::PingRequest => {
                let ping_request = PingRequest::read_fully(&mut bytes)?;
                let pong_response = PongResponse {
                    time: ping_request.time
                };

                pong_response.write_packet(&mut client_state.packet_buffer);

                client_state.connection.borrow_mut().send(&mut client_state.packet_buffer);

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

                match client_state.authentication {
                    AuthenticationMode::None => {
                        // Send login success
                        let login_success = login::clientbound::LoginSuccess {
                            profile: GameProfile {
                                uuid: hello.uuid,
                                username: Cow::Borrowed(hello.username.borrow()),
                                properties: vec![],
                            }
                        };
                        login_success.write_packet(&mut client_state.packet_buffer);
                        client_state.connection.borrow_mut().send(&mut client_state.packet_buffer);

                        // Change phase
                        client_state.phase = Phase::LoginWaitForAck;
                    },
                    AuthenticationMode::Velocity(_) => {
                        client_state.query_transaction_id = rand::thread_rng().gen();

                        // Send query
                        let query = login::clientbound::CustomQuery {
                            transaction_id: client_state.query_transaction_id,
                            channel: Cow::Borrowed("velocity:player_info"),
                            payload: Cow::Borrowed(&[])
                        };
                        query.write_packet(&mut client_state.packet_buffer);
                        client_state.connection.borrow_mut().send(&mut client_state.packet_buffer);

                        // Change phase
                        client_state.phase = Phase::LoginWaitForVelocityResponse;
                    },
                }

                // Save information
                client_state.profile = Some(GameProfile {
                    uuid: hello.uuid,
                    username: Cow::Owned(hello.username.to_string()),
                    properties: Vec::new(),
                });

                Ok(HandleAction::Continue)
            },
            _ => Ok(HandleAction::Disconnect)
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
            login::serverbound::PacketId::LoginAcknowledged => {
                let Some(profile) = client_state.profile.take() else {
                    return Ok(HandleAction::Disconnect);
                };

                // Redirect connection
                let concierge = unsafe { client_state.concierge.as_mut() }.unwrap();
                let login_information = LoginInformation {
                    profile
                };

                Ok(HandleAction::Transfer(Box::new(move |stream: SendableConnection| {
                    (concierge.sender)(login_information, stream);
                })))
            }
            _ => Ok(HandleAction::Disconnect)
        }
    } else {
        bail!(
            "Unknown packet_id {} during login",
            packet_id
        );
    }
}

slice_serializable! {
    #[derive(Debug)]
    pub struct VelocitySignedData<'a> {
        pub version: i32 as VarInt,
        pub address: Cow<'a, str> as SizedString,
        pub profile: GameProfile<'a>
    }
}

slice_serializable! {
    #[derive(Debug)]
    pub struct VelocityQueryData<'a> {
        pub signature: &'a [u8] as FixedBlob<32>,
        pub signed_data: &'a [u8] as GreedyBlob
    }
}

slice_serializable! {
    #[derive(Debug)]
    pub struct VelocityQueryAnswer<'a> {
        pub transaction_id: i32 as VarInt,
        pub payload: Option<VelocityQueryData<'a>>
    }
}

type HmacSha256 = Hmac<Sha256>;

fn handle_login_wait_for_velocity_response(client_state: &mut ClientState, mut bytes: &[u8]) -> anyhow::Result<HandleAction> {
    let packet_id: u8 = Single::read(&mut bytes)?;
    if let Ok(packet_id) = login::serverbound::PacketId::try_from(packet_id) {
        match packet_id {
            login::serverbound::PacketId::CustomQueryAnswer => {
                let answer = VelocityQueryAnswer::read_fully(&mut bytes)?;

                if answer.transaction_id != client_state.query_transaction_id {
                    return Ok(HandleAction::Disconnect);
                }

                let Some(data) = answer.payload else {
                    return Ok(HandleAction::Disconnect);
                };

                let AuthenticationMode::Velocity(key) = &client_state.authentication else {
                    return Ok(HandleAction::Disconnect);
                };

                let mut mac = HmacSha256::new_from_slice(&*key).unwrap();
                mac.update(data.signed_data);
                
                if let Ok(_) = mac.verify_slice(data.signature) {
                    let mut bytes = data.signed_data;
                    let signed_data = VelocitySignedData::read_fully(&mut bytes)?;

                    let Some(old_profile) = client_state.profile.take() else {
                        return Ok(HandleAction::Disconnect);
                    };

                    if signed_data.profile.uuid != old_profile.uuid {
                        return Ok(HandleAction::Disconnect);
                    }
                    if signed_data.profile.username != old_profile.username {
                        return Ok(HandleAction::Disconnect);
                    }

                    let mut properties = Vec::new();
                    for property in &signed_data.profile.properties {
                        properties.push(GameProfileProperty {
                            id: Cow::Owned(property.id.to_string()),
                            value: Cow::Owned(property.value.to_string()),
                            signature: property.signature.as_ref().map(|signature| Cow::Owned(signature.to_string())),
                        })
                    }

                    let owned_profile = GameProfile {
                        uuid: signed_data.profile.uuid,
                        username: Cow::Owned(signed_data.profile.username.to_string()),
                        properties,
                    };

                    // Send login success
                    let login_success = login::clientbound::LoginSuccess {
                        profile: signed_data.profile
                    };
                    login_success.write_packet(&mut client_state.packet_buffer);
                    client_state.connection.borrow_mut().send(&mut client_state.packet_buffer);

                    // Change phase
                    client_state.profile = Some(owned_profile);
                    client_state.phase = Phase::LoginWaitForAck;

                    Ok(HandleAction::Continue)
                } else {
                    Ok(HandleAction::Disconnect)
                }
            }
            _ => Ok(HandleAction::Disconnect)
        }
    } else {
        bail!(
            "Unknown packet_id {} during login",
            packet_id
        );
    }
}