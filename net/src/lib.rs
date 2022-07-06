use std::net::TcpStream;

pub mod network_buffer;
pub mod network_handler;
pub mod packet_helper;

#[derive(Debug)]
pub enum ConnectionState {
    Handshake,
    Status,
    Login,
    Play,
}

pub struct PlayerConnection {
    pub stream: TcpStream,
    pub state: ConnectionState,
    pub closed: bool,
}

impl PlayerConnection {
    pub fn close(&mut self) {
        if !self.closed {
            let _ = self.stream.shutdown(std::net::Shutdown::Both);
            self.closed = true;
        }
    }
}
