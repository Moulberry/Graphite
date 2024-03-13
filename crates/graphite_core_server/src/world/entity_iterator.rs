use std::{cell::UnsafeCell, marker::PhantomData, rc::Rc};

use crate::{entity::{Entity, EntityExtension, GenericEntity}, player::{GenericPlayer, Player, PlayerExtension}};

type PlayerIter<'a> = slab::Iter<'a, Rc<UnsafeCell<dyn GenericPlayer>>>;
type PlayerIterMut<'a> = slab::IterMut<'a, Rc<UnsafeCell<dyn GenericPlayer>>>;
type EntityIter<'a> = slab::Iter<'a, Rc<UnsafeCell<dyn GenericEntity>>>;
type EntityIterMut<'a> = slab::IterMut<'a, Rc<UnsafeCell<dyn GenericEntity>>>;

pub struct PlayerIterator<'a, P: PlayerExtension> {
    players: PlayerIter<'a>,
    empty: bool,
    phantom: PhantomData<P>
}

impl <'a, P: PlayerExtension> PlayerIterator<'a, P> {
    pub fn new(players: PlayerIter<'a>, empty: bool) -> Self {
        Self {
            players,
            empty,
            phantom: PhantomData
        }
    }
}

impl <'a, P: PlayerExtension> Iterator for PlayerIterator<'a, P> {
    type Item = &'a Player<P>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.empty {
            return None;
        }

        loop {
            if let Some((_, player)) = self.players.next() {
                let player = unsafe { player.get().as_ref().unwrap() };
                
                let downcasted: Option<&Player<P>> = player.downcast_ref();
                if let Some(downcasted) = downcasted {
                    if downcasted.is_valid() {
                        return Some(downcasted);
                    }
                }
            } else {
                self.empty = true;
                return None;
            }
        }
    }
}

pub struct PlayerIteratorMut<'a, P: PlayerExtension> {
    players: PlayerIterMut<'a>,
    empty: bool,
    phantom: PhantomData<P>
}

impl <'a, P: PlayerExtension> PlayerIteratorMut<'a, P> {
    pub fn new(players: PlayerIterMut<'a>, empty: bool) -> Self {
        Self {
            players,
            empty,
            phantom: PhantomData
        }
    }
}

impl <'a, P: PlayerExtension> Iterator for PlayerIteratorMut<'a, P> {
    type Item = &'a mut Player<P>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.empty {
            return None;
        }

        loop {
            if let Some((_, player)) = self.players.next() {
                let player = unsafe { player.get().as_mut().unwrap() };

                let downcasted: Option<&mut Player<P>> = player.downcast_mut();
                if let Some(downcasted) = downcasted {
                    if downcasted.is_valid() {
                        return Some(downcasted);
                    }
                }
            } else {
                self.empty = true;
                return None;
            }
        }
    }
}

pub struct EntityIterator<'a, E: EntityExtension> {
    entities: EntityIter<'a>,
    empty: bool,
    phantom: PhantomData<E>
}

impl <'a, E: EntityExtension> EntityIterator<'a, E> {
    pub fn new(entities: EntityIter<'a>, empty: bool) -> Self {
        Self {
            entities,
            empty,
            phantom: PhantomData
        }
    }
}

impl <'a, E: EntityExtension> Iterator for EntityIterator<'a, E> {
    type Item = &'a Entity<E>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.empty {
            return None;
        }

        loop {
            if let Some((_, entity)) = self.entities.next() {
                let entity = unsafe { entity.get().as_ref().unwrap() };
                
                let downcasted: Option<&Entity<E>> = entity.downcast_ref();
                if let Some(downcasted) = downcasted {
                    if downcasted.self_id.is_some() {
                        return Some(downcasted);
                    }
                }
            } else {
                self.empty = true;
                return None;
            }
        }
    }
}

pub struct EntityIteratorMut<'a, E: EntityExtension> {
    entities: EntityIterMut<'a>,
    empty: bool,
    phantom: PhantomData<E>
}

impl <'a, E: EntityExtension> EntityIteratorMut<'a, E> {
    pub fn new(entities: EntityIterMut<'a>, empty: bool) -> Self {
        Self {
            entities,
            empty,
            phantom: PhantomData
        }
    }
}

impl <'a, E: EntityExtension> Iterator for EntityIteratorMut<'a, E> {
    type Item = &'a mut Entity<E>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.empty {
            return None;
        }

        loop {
            if let Some((_, entity)) = self.entities.next() {
                let entity = unsafe { entity.get().as_mut().unwrap() };
                
                let downcasted: Option<&mut Entity<E>> = entity.downcast_mut();
                if let Some(downcasted) = downcasted {
                    if downcasted.self_id.is_some() {
                        return Some(downcasted);
                    }
                }
            } else {
                return None;
            }
        }
    }
}