use sticky::Unsticky;

use crate::{
    entity::position::Position,
    error::UninitializedError,
    world::{TickPhase, World},
};

use super::{
    player::{Player, PlayerService},
    proto_player::ProtoPlayer,
};

pub struct PlayerVec<P: PlayerService> {
    players: sticky::StickyVec<Player<P>>,
    world: *mut World<P::WorldServiceType>,
}

impl<P: PlayerService> Default for PlayerVec<P> {
    fn default() -> Self {
        Self {
            players: Default::default(),
            world: std::ptr::null_mut(),
        }
    }
}

impl<P: PlayerService> PlayerVec<P> {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn initialize(&self, world: &World<P::WorldServiceType>) {
        // Justification:
        // If the world pointer is null, this struct is in an undefined state
        // Therefore, any reference that previously existed to this struct
        // is invalid, so converting the immutable reference to a mutable one
        // should be sound here
        unsafe {
            let self_mut: *mut PlayerVec<P> = self as *const _ as *mut _;
            let self_mut_ref: &mut PlayerVec<P> = self_mut.as_mut().unwrap();
            assert!(self_mut_ref.world.is_null(), "cannot initialize twice");
            self_mut_ref.world = world as *const _ as *mut _;
        }
    }

    pub fn get_world(&self) -> anyhow::Result<&mut World<P::WorldServiceType>> {
        unsafe { self.world.as_mut() }.ok_or_else(|| UninitializedError.into())
    }

    pub fn get_by_index(&self, index: usize) -> &Player<P> {
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

        let world = self.get_world()?;
        let player = proto_player.create_player(service, world, position)?;
        self.players.push(player);

        Ok(())
    }

    pub fn remove(&mut self, index: usize) -> <Player<P> as Unsticky>::UnstuckType {
        self.players.remove(index)
    }

    pub fn len(&self) -> usize {
        self.players.len()
    }

    pub fn is_empty(&self) -> bool {
        self.players.is_empty()
    }

    pub fn tick(&mut self, tick_phase: TickPhase) {
        self.players
            .retain_mut(|player| player.tick(tick_phase).is_ok());
    }
}
