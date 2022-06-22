mod server_join_game;

pub use server_join_game::ServerJoinGame;

use crate::packet::IdentifiedPacket;
use derive_try_from_primitive::TryFromPrimitive;
use super::identify_packets;

/*identify_packets!(
    ClientPacketId,
    ClientLoginStart = 0x00
);*/

identify_packets!(
    ServerPacketId,
    ServerJoinGame = 0x23
);
