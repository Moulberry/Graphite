mod server_response;

pub use server_response::ServerResponse;

use super::identify_packets;

identify_packets!(
    ServerResponse = 0
);