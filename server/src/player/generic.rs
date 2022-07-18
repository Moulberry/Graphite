use protocol::play::server;
use text_component::TextComponent;

use super::{Player, PlayerService};

pub trait DynamicPlayer {
    fn send_message(&mut self, message: &TextComponent);
    fn disconnect(&mut self);
}

impl<P: PlayerService> DynamicPlayer for Player<P> {
    fn send_message(&mut self, message: &TextComponent) {
        // self.service.whatever;

        self.write_packet(&server::SystemChat {
            message: message.to_json(),
            overlay: false,
        })
    }

    fn disconnect(&mut self) {
        self.disconnected = true;
    }
}