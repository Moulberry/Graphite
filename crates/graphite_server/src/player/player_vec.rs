use crate::{
    entity::position::Position,
    error::UninitializedError,
    world::{TickPhase, World, TickPhaseInner},
};

use super::{
    player::{Player, PlayerService},
    proto_player::ProtoPlayer,
};

pub struct PlayerVec<P: PlayerService> {
    players: graphite_sticky::StickyVec<Player<P>>,
    locked: bool,
    delayed_add: Vec<(ProtoPlayer<P::UniverseServiceType>, P, Position)>,
    world: *mut World<P::WorldServiceType>,
}

impl<P: PlayerService> Default for PlayerVec<P> {
    fn default() -> Self {
        Self {
            players: Default::default(),
            locked: false,
            delayed_add: Default::default(),
            world: std::ptr::null_mut(),
        }
    }
}

impl<P: PlayerService> PlayerVec<P> {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn update_world_ptr(&mut self, world: *mut World<P::WorldServiceType>) {
        self.world = world;

        // Update all the world refs of the players inside this player vec
        for player in self.players.iter_mut() {
            player.world = self.world;
        }
    }

    pub fn get_world(&self) -> anyhow::Result<&mut World<P::WorldServiceType>> {
        unsafe { self.world.as_mut() }.ok_or_else(|| UninitializedError.into())
    }

    pub fn get_by_index(&self, index: usize) -> Option<&Player<P>> {
        self.players.get(index)
    }

    pub fn add(
        &mut self,
        proto_player: ProtoPlayer<P::UniverseServiceType>,
        service: P,
        position: Position,
    ) -> anyhow::Result<()> {
        if self.world.is_null() {
            return Err(UninitializedError.into());
        }

        if self.locked {
            self.delayed_add.push((proto_player, service, position));
            return Ok(());
        }

        let world = self.get_world()?;
        let player = proto_player.create_player(service, world, position)?;
        self.players.push(player);

        Ok(())
    }

    pub fn len(&self) -> usize {
        self.players.len()
    }

    pub fn is_empty(&self) -> bool {
        self.players.is_empty()
    }

    pub fn tick(&mut self, tick_phase: TickPhase) {
        if let Some(world) = unsafe { self.world.as_mut() } {
            match tick_phase.0 {
                TickPhaseInner::Update => {
                    self.locked = true;
                    self.tick_players_update(tick_phase, world);
                    self.locked = false;
                }
                TickPhaseInner::View => {
                    self.locked = true;
                    self.players
                        .retain_mut(|player| player.tick(tick_phase).is_ok());
                    self.locked = false;

                    // It is important that new players are added after View,
                    // otherwise a player may see themselves despawn
                    for (proto_player, service, position) in self.delayed_add.drain(..) {
                        if let Ok(player) = proto_player.create_player(service, world, position) {
                            self.players.push(player);
                        }
                    }
                }
            }
        }        
    }

    fn tick_players_update(&mut self, tick_phase: TickPhase, world: &mut World<<P as PlayerService>::WorldServiceType>) {
        let filter = |player: &mut Player<P>| {
            player.tick(tick_phase).is_err() || player.transfer_fn.is_some()
        };
        
        for unstuck in self.players.drain_filter(filter) {
            if let Some((proto_player, old_service, transfer_fn)) = unstuck {
                if let Some(transfer_fn) = transfer_fn {
                    transfer_fn(world, old_service, proto_player)
                }
            }
        }
    }
}
