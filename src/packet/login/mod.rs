mod client_login_start;
mod server_login_success;

pub use client_login_start::ClientLoginStart;
pub use server_login_success::ServerLoginSuccess;

use crate::packet::IdentifiedPacket;
use derive_try_from_primitive::TryFromPrimitive;
use super::identify_packets;

identify_packets!(
    ClientPacketId,
    ClientLoginStart = 0x00
);

identify_packets!(
    ServerPacketId,
    ServerLoginSuccess = 0x02
);
