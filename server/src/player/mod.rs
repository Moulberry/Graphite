pub mod player_connection;
pub mod player_settings;
pub mod player_vec;
pub mod proto_player;

mod player_packet_handler;

#[allow(clippy::module_inception)]
// Justification: we re-export player, moving all the `player.rs` code to here would be messy
mod player;
pub use player::*;
