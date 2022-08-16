// GameMode

/*pub struct GameMode {
    pub id: u8,
    pub invulnerable: bool,
    pub force_flying: Option<bool>,
    pub allow_flying: bool,
    pub instant_breaking: bool,
    pub unrestricted_building: bool
}

const CREATIVE: GameMode = GameMode {

};*/

use net::packet_helper;
use protocol::play::server::{
    GameEvent, GameEventType, PlayerAbilities, PlayerInfo, PlayerInfoUpdateGamemode,
};

use crate::player::{Player, PlayerService};

#[derive(Default, PartialEq, Eq, Clone, Copy)]
#[repr(u8)]
pub enum GameMode {
    #[default]
    Survival,
    Creative,
    Adventure,
    Spectator,
}

// PlayerAbilities
#[readonly::make]
pub struct Abilities {
    dirty: bool,

    last_gamemode: GameMode,
    pub gamemode: GameMode,

    #[readonly]
    pub invulnerable: bool,
    #[readonly]
    pub is_flying: bool,
    #[readonly]
    pub allow_flying: bool,
    #[readonly]
    pub instant_breaking: bool,
    #[readonly]
    pub unrestricted_building: bool,
    #[readonly]
    pub flying_speed: f32,
    #[readonly]
    pub walking_speed: f32,
}

impl Default for Abilities {
    fn default() -> Self {
        Self {
            dirty: false,
            last_gamemode: Default::default(),
            gamemode: Default::default(),
            invulnerable: false,
            is_flying: false,
            allow_flying: false,
            instant_breaking: false,
            unrestricted_building: true,
            flying_speed: 0.05,
            walking_speed: 0.1,
        }
    }
}

impl Abilities {
    pub(crate) fn write_changes<P: PlayerService>(player: &mut Player<P>) {
        let abilities = &mut player.abilities;

        if abilities.last_gamemode != abilities.gamemode {
            abilities.last_gamemode = abilities.gamemode;

            // Update abilities based on the new gamemode
            match abilities.gamemode {
                GameMode::Creative => {
                    abilities.allow_flying = true;
                    abilities.instant_breaking = true;
                    abilities.invulnerable = true;
                }
                GameMode::Spectator => {
                    abilities.allow_flying = true;
                    abilities.instant_breaking = false;
                    abilities.invulnerable = true;
                    abilities.is_flying = true;
                }
                _ => {
                    abilities.allow_flying = false;
                    abilities.instant_breaking = false;
                    abilities.invulnerable = false;
                    abilities.is_flying = false;
                }
            }

            // Send game mode change
            packet_helper::write_packet(
                &mut player.write_buffer,
                &abilities.create_set_gamemode_packet(),
            )
            .expect("packet larger than 2MB");

            // Send gamemode change for player info
            let player_info_change_gamemode = PlayerInfo::UpdateGameMode {
                values: vec![PlayerInfoUpdateGamemode {
                    uuid: player.profile.uuid,
                    gamemode: abilities.gamemode as u8,
                }],
            };
            packet_helper::write_packet(&mut player.write_buffer, &player_info_change_gamemode)
                .expect("packet larger than 2MB");

            // Send abilities
            packet_helper::write_packet(
                &mut player.write_buffer,
                &abilities.create_abilities_packet(),
            )
            .expect("packet larger than 2MB");
            abilities.dirty = false;

            // Additional packets that the client expects
            if abilities.gamemode == GameMode::Spectator {
                // todo: Set player invisible
            }
        } else if abilities.dirty {
            // Send abilities
            packet_helper::write_packet(
                &mut player.write_buffer,
                &abilities.create_abilities_packet(),
            )
            .expect("packet larger than 2MB");
            abilities.dirty = false;
        }
    }

    pub fn create_abilities_packet(&self) -> PlayerAbilities {
        PlayerAbilities {
            invulnerable: self.invulnerable,
            is_flying: self.is_flying,
            allow_flying: self.allow_flying,
            instant_breaking: self.instant_breaking,
            flying_speed: self.flying_speed,
            walking_speed: self.walking_speed,
        }
    }

    pub fn create_set_gamemode_packet(&self) -> GameEvent {
        GameEvent {
            event_type: GameEventType::ChangeGameMode,
            param: self.gamemode as u8 as f32,
        }
    }
}
