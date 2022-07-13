use sticky::Unsticky;

use crate::{
    error::UninitializedError,
    world::World, position::Position,
};

use super::{player::{PlayerService, Player}, proto_player::ProtoPlayer};

pub struct PlayerVec<P: PlayerService> {
    players: sticky::StickyVec<Player<P>>,
    world: *mut World<P::WorldServiceType>,
}

impl<P: PlayerService> PlayerVec<P> {
    pub fn new() -> Self {
        Self {
            players: Default::default(),
            world: std::ptr::null_mut(),
        }
    }

    pub fn initialize(&self, world: &World<P::WorldServiceType>) {
        // todo: justify this
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

    pub fn add(
        &mut self,
        proto_player: ProtoPlayer<P::UniverseServiceType>,
        service: P,
        position: Position,
    ) -> anyhow::Result<()> {
        let world = self.get_world()?;
        let player = proto_player.create_player(service, world, position)?;
        self.players.insert(player);
        Ok(())
    }

    pub fn remove(&mut self, index: usize) -> <Player<P> as Unsticky>::UnstuckType {
        self.players.remove(index)
    }

    pub fn len(&self) -> usize {
        self.players.len()
    }

    pub fn tick(&mut self) {
        self.players.retain_mut(|player| player.tick().is_ok());
    }
}
