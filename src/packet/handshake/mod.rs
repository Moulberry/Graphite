mod client_handshake;

pub use client_handshake::ClientHandshake;

use crate::packet::IdentifiedPacket;
use derive_try_from_primitive::TryFromPrimitive;
use super::identify_packets;

identify_packets!(
    ClientPacketId,
    ClientHandshake<'_> = 0x00
);