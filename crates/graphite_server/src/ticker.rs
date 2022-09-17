use crate::{universe::{Universe, UniverseService}, world::{World, WorldService, TickPhase}};

pub unsafe trait WorldTicker<W: WorldService> {
    fn update_universe_ptr(&mut self, universe: *mut Universe<W::UniverseServiceType>);
    fn update_children_ptr(&mut self, world: *mut World<W>);
    fn tick(&mut self, tick_phase: TickPhase);
}

pub unsafe trait UniverseTicker<U: UniverseService> {
    fn update_children_ptr(&mut self, universe: *mut Universe<U>);
    fn tick(&mut self);
}