use std::{cell::UnsafeCell, marker::PhantomData, rc::Rc};

use glam::DVec3;

use crate::player::{GenericPlayer, Player, PlayerExtension};

use super::chunk_iterator::{NearbyChunkIter, NearbyChunkIterMut};


type PlayerIter<'a> = slab::Iter<'a, Rc<UnsafeCell<dyn GenericPlayer>>>;
type PlayerIterMut<'a> = slab::IterMut<'a, Rc<UnsafeCell<dyn GenericPlayer>>>;

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
                if let Some(value) = downcast_player(player) {
                    return Some(value);
                }
            } else {
                self.empty = true;
                return None;
            }
        }
    }
}

fn downcast_player<P: PlayerExtension>(player: &Rc<UnsafeCell<dyn GenericPlayer>>) -> Option<&Player<P>> {
    let player = unsafe { player.get().as_ref().unwrap() };
    let downcasted: Option<&Player<P>> = player.downcast_ref();
                
    if let Some(downcasted) = downcasted {
        if downcasted.is_still_connected() {
            return Some(downcasted);
        }
    }
    None
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
                    if downcasted.is_still_connected() {
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

pub struct NearbyPlayerIterator<'a, P: PlayerExtension> {
    pub(crate) chunks: NearbyChunkIter<'a>,
    pub(crate) chunk_players: Option<PlayerIterator<'a, P>>,
    pub(crate) position: DVec3,
    pub(crate) distance_sq: f64,
    pub(crate) phantom: PhantomData<P>
}

impl <'a, P: PlayerExtension> Iterator for NearbyPlayerIterator<'a, P> {
    type Item = &'a Player<P>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(players) = &mut self.chunk_players {
            while let Some(player) = players.next() {
                if player.position.distance_squared(self.position) < self.distance_sq {
                    return Some(player);
                }
            }
            self.chunk_players = None;
        }

        loop {
            let chunk = self.chunks.next()?;
            let mut player_iterator = chunk.players();

            while let Some(player) = player_iterator.next() {
                if player.position.distance_squared(self.position) < self.distance_sq {
                    self.chunk_players = Some(player_iterator);
                    return Some(player);
                }
            }
        }
    }
}

pub struct NearbyPlayerIterMut<'a, P: PlayerExtension> {
    pub(crate) chunks: NearbyChunkIterMut<'a>,
    pub(crate) chunk_players: Option<PlayerIteratorMut<'a, P>>,
    pub(crate) position: DVec3,
    pub(crate) distance_sq: f64,
    pub(crate) phantom: PhantomData<P>
}

impl <'a, P: PlayerExtension> Iterator for NearbyPlayerIterMut<'a, P> {
    type Item = &'a mut Player<P>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(players) = &mut self.chunk_players {
            while let Some(player) = players.next() {
                if player.position.distance_squared(self.position) < self.distance_sq {
                    return Some(player);
                }
            }
            self.chunk_players = None;
        }

        loop {
            let chunk = self.chunks.next()?;
            let mut player_iterator = chunk.players_mut();

            while let Some(player) = player_iterator.next() {
                if player.position.distance_squared(self.position) < self.distance_sq {
                    self.chunk_players = Some(player_iterator);
                    return Some(player);
                }
            }
        }
    }
}