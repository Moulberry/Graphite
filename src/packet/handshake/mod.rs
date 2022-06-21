mod client_handshake;

pub use client_handshake::ClientHandshake;

use super::identify_packets;

identify_packets!(
    ClientHandshake = 0
);