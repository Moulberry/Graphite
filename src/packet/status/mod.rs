mod server_response;

pub use server_response::ServerResponse;

use crate::packet::IdentifiedPacket;
use derive_try_from_primitive::TryFromPrimitive;
use super::identify_packets;

identify_packets!(
    ServerPacketId,
    ServerResponse<'_> = 0x00
);