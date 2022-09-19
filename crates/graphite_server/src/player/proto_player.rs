use crate::{
    entity::position::Position,
    gamemode::Abilities,
    universe::{EntityId, UniverseService},
    world::World,
};
use graphite_net::{network_buffer::WriteBuffer, packet_helper};
use graphite_mc_protocol::{
    play::server::{PlayerInfo, PlayerInfoAddPlayer, Respawn},
    types::GameProfile,
};

use super::{
    player::{Player, PlayerService},
    player_connection::AbstractConnectionReference,
};

// Proto player

pub struct ProtoPlayer<U: UniverseService> {
    pub(crate) connection: U::ConnectionReferenceType,
    pub hardcore: bool,
    pub abilities: Abilities,
    pub profile: GameProfile,

    pub(crate) write_buffer: WriteBuffer,
    pub(crate) entity_id: EntityId,
}

impl<U: UniverseService> ProtoPlayer<U> {
    pub fn new(
        connection: U::ConnectionReferenceType,
        profile: GameProfile,
        entity_id: EntityId,
    ) -> Self {
        Self {
            hardcore: false,
            abilities: Default::default(),
            profile,

            write_buffer: WriteBuffer::new(),
            entity_id,

            connection,
        }
    }

    pub(crate) fn create_player<P: PlayerService<UniverseServiceType = U>>(
        mut self,
        service: P,
        world: &mut World<P::WorldServiceType>,
        position: Position,
    ) -> anyhow::Result<Player<P>> {
        // Fill write buffer with required initial packets

        // todo: dont send all these packets if the player is in the same world
        // i.e. the player had it's PlayerService changed
        // holdup: implementing dimension ids to be able to differentiate worlds

        // Send player info
        let add_player_info = PlayerInfo::AddPlayer {
            values: vec![PlayerInfoAddPlayer {
                profile: self.profile.clone(),
                gamemode: self.abilities.gamemode as u8,
                ping: 0,
                display_name: None,
                signature_data: None,
            }],
        };
        packet_helper::try_write_packet(&mut self.write_buffer, &add_player_info);

        /*let respawn = Respawn {
            dimension_type: "graphite:default_dimension",
            dimension_name: "graphite:default_dimension",
            hashed_seed: 0,
            gamemode: self.abilities.gamemode as u8,
            previous_gamemode: -1,
            is_debug: false,
            is_flat: false,
            copy_metadata: false,
            death_location: None,
        };
        packet_helper::try_write_packet(&mut self.write_buffer, &respawn);

        let respawn = Respawn {
            dimension_type: "graphite:default_dimension2",
            dimension_name: "graphite:default_dimension2",
            hashed_seed: 0,
            gamemode: self.abilities.gamemode as u8,
            previous_gamemode: -1,
            is_debug: false,
            is_flat: false,
            copy_metadata: false,
            death_location: None,
        };
        packet_helper::try_write_packet(&mut self.write_buffer, &respawn);*/

        // todo: if dim changed, send dimension changed
        // todo: else, don't send

        // todo: send new render distance

        let view_position = world.initialize_view_position(&mut self, position);

        // Write the necessary packets to the TCP stream
        self.connection.write_bytes(self.write_buffer.pop_written());

        let player = Player::new(service, world, position, view_position, self);

        Ok(player)
    }
}
