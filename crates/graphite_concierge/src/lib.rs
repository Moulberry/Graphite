use std::cell::RefCell;
use std::collections::HashMap;
use std::net::ToSocketAddrs;
use std::rc::Rc;

use anyhow::bail;
use graphite_binary::slice_serialization::{Single, SliceSerializable};
use graphite_mc_protocol::{handshake};
use graphite_mc_protocol::handshake::client::Intention;
use graphite_network::{NetworkHandlerService, Connection, FramedPacketHandler};
use message_io::network::{ResourceId};
use slab::Slab;

struct Concierge {
    client_states: HashMap<ResourceId, ClientState>
}

enum Phase {
    Initial,
    Status,
    Login
}

const BUFFER_SIZE: usize = 2097148;

struct ClientState {
    connection: Rc<RefCell<Connection<ConciergeNetworkHandlerService>>>,
    phase: Phase,

    protocol_version: i32,
    connected_host: String,
    connected_port: u16,

    received_status: bool,

    idx: Option<usize>
}

impl FramedPacketHandler<ConciergeNetworkHandlerService> for ClientState {
    fn handle(&mut self, net: &mut ConciergeNetworkHandlerService, data: &[u8]) {
        println!("Client state: received {}", String::from_utf8_lossy(data));
    }

    fn disconnected(&mut self, net: &mut ConciergeNetworkHandlerService) {
        println!("Client state: disconnected");
        net.client_states.remove(self.idx.unwrap());

    }
}

#[derive(Default)]
struct ConciergeNetworkHandlerService {
    client_states: Slab<Rc<RefCell<ClientState>>>
}

impl NetworkHandlerService for ConciergeNetworkHandlerService {
    fn accept_new_connection(&mut self, connection: Rc<RefCell<Connection<ConciergeNetworkHandlerService>>>) {
        let state = Rc::new(RefCell::new(ClientState {
            connection: connection.clone(),
            phase: Phase::Initial,
            protocol_version: 0,
            connected_host: String::new(),
            connected_port: 0,
            received_status: false,
            idx: None
        }));

        let idx = self.client_states.insert(state.clone());
        state.borrow_mut().idx = Some(idx);

        connection.borrow_mut().set_handler(state.clone());

        println!("Got new connection, total: {}", self.client_states.len());
    }
}

pub fn listen(addr: impl ToSocketAddrs) {
    let mut handler = graphite_network::NetworkHandler::new(
        ConciergeNetworkHandlerService::default()
    );
    handler.listen(addr).unwrap();
}

// // Read incoming network events.
// listener.for_each(move |event| match event {
//     node::NodeEvent::Network(net_event) => match net_event {
//         NetEvent::Connected(_, _) => unreachable!(),
//         NetEvent::Accepted(endpoint, _id) => {
//             concierge.client_states.insert(endpoint.resource_id(), ClientState::default());

//             println!("Client connected: {}", endpoint.resource_id());
//             handler.signals().send_with_timer(Signal::TimeoutHandshake(endpoint.resource_id()), Duration::from_secs(10));
//         },
//         NetEvent::Message(endpoint, mut data) => {
//             println!("Received {} ({} bytes): {}", endpoint.resource_id(), data.len(), String::from_utf8_lossy(data));

//             if let Some(client_state) = concierge.client_states.get_mut(&mut endpoint.resource_id()) {
//                 let result = match client_state.phase {
//                     Phase::Initial => {
//                         handle_intention(client_state, &mut data)
//                     },
//                     Phase::Status => {
//                         handle_status(&handler, endpoint, &mut buffer, client_state, &mut data)
//                     },
//                     Phase::Login => todo!(),
//                 };
//                 match result {
//                     Ok(disconnect) => if disconnect {
//                         concierge.client_states.remove(&endpoint.resource_id());
//                         handler.network().remove(endpoint.resource_id());
//                     },
//                     Err(error) => if cfg!(debug_assertions) {
//                         panic!("Encountered error in Concierge: {}", error);
//                     } else {
//                         concierge.client_states.remove(&endpoint.resource_id());
//                         handler.network().remove(endpoint.resource_id());
//                     },
//                 }
//             } else {
//                 handler.network().remove(endpoint.resource_id());
//             }
//         },
//         NetEvent::Disconnected(endpoint) => {
//             concierge.client_states.remove(&endpoint.resource_id());
//             println!("Client disconnected: {}", endpoint.resource_id()) // Tcp or Ws
//         },
//     },
//     node::NodeEvent::Signal(signal) => match signal {
//         Signal::TimeoutHandshake(resource_id) => {
//             concierge.client_states.remove(&resource_id);
//             let removed = handler.network().remove(resource_id);
//             println!("Tried to remove: {}, {}", resource_id, removed) // Tcp or Ws
//         }
//     },
// });

fn handle_intention(client_state: &mut ClientState, bytes: &mut &[u8]) -> anyhow::Result<bool> {
    if bytes.len() < 3 {
        bail!("Insufficient bytes for handshake");
    } else if bytes[0..3] == [0xFE, 0x01, 0xFA] {
        bail!("Legacy server list ping is not supported");
    } else {
        let packet_id: u8 = Single::read(bytes)?;
        if let Ok(packet_id) = handshake::client::PacketId::try_from(packet_id) {
            match packet_id {
                handshake::client::PacketId::Intention => {
                    let intention_packet = Intention::read_fully(bytes)?;
                    println!("Read intention packet: {:?}", intention_packet);

                    match intention_packet.intention {
                        handshake::client::IntentionType::Status => {
                            client_state.connected_host = intention_packet.host_name.to_string();
                            client_state.connected_port = intention_packet.port;
                            client_state.protocol_version = intention_packet.protocol_version;
                            client_state.phase = Phase::Status;
                        },
                        handshake::client::IntentionType::Login => {
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

    Ok(false)
}

// fn handle_status(node_handler: &NodeHandler<Signal>, endpoint: Endpoint, buffer: &mut Vec<u8>,
//         client_state: &mut ClientState, bytes: &mut &[u8]) -> anyhow::Result<bool> {
//     let packet_id: u8 = Single::read(bytes)?;
//     if let Ok(packet_id) = status::client::PacketId::try_from(packet_id) {
//         match packet_id {
//             status::client::PacketId::StatusRequest => {
//                 if client_state.received_status {
//                     return Ok(true);
//                 }

//                 let status_response = StatusResponse {
//                     json: r#"{
//                         "version": {
//                             "name": "1.20.4",
//                             "protocol": 765
//                         },
//                         "players": {
//                             "max": 100,
//                             "online": 5,
//                             "sample": [
//                                 {
//                                     "name": "thinkofdeath",
//                                     "id": "4566e69f-c907-48ee-8d71-d7ba5aa00d20"
//                                 }
//                             ]
//                         },
//                         "description": {
//                             "text": "Hello world"
//                         },
//                         "favicon": "data:image/png;base64,<data>",
//                         "enforcesSecureChat": true,
//                         "previewsChat": true
//                     }"#,
//                 };

//                 send_packet(node_handler, endpoint, buffer, &status_response)?;
//             }
//             status::client::PacketId::PingRequest => {
//                 let ping_request = PingRequest::read_fully(bytes)?;
//                 let pong_response = PongResponse {
//                     time: ping_request.time
//                 };

//                 send_packet(node_handler, endpoint, buffer, &pong_response)?;

//                 return Ok(true);
//             }
//         }
//     } else {
//         bail!(
//             "Unknown packet_id {} during status",
//             packet_id
//         );
//     }

//     Ok(false)
// }

// fn send_packet<'a, I: std::fmt::Debug, T>(
//     node_handler: &NodeHandler<Signal>, endpoint: Endpoint, buffer: &mut Vec<u8>,
//     packet: &'a T,
// ) -> anyhow::Result<()>
// where
//     T: SliceSerializable<'a, T> + IdentifiedPacket<I> + 'a,
// {
//     let write_size = T::get_write_size(T::as_copy_type(&packet));
//     if write_size > BUFFER_SIZE {
//         bail!("Packet too large");
//     }

//     let buffer_len = buffer.len();
//     let slice_after_writing = unsafe { T::write(&mut buffer[1..], T::as_copy_type(&packet)) };
//     let bytes_written = buffer_len - slice_after_writing.len();

//     buffer[0] = packet.get_packet_id_as_u8();

//     let data = &buffer[0..bytes_written];
//     node_handler.network().send(endpoint, data);

//     Ok(())
// }