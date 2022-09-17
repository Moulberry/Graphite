use crate::universe::Universe;
use super::{World, WorldService};

pub struct WorldMap<K, W: WorldService> {
    worlds: graphite_sticky::StickyMap<K, World<W>>,
    universe: *mut Universe<W::UniverseServiceType>,
    parent_world: *mut World<W::ParentWorldServiceType>,
    locked: bool,
    delayed_insert: Vec<(K, World<W>)>,
    delayed_remove: Vec<K>
}

impl<K, W: WorldService> Default for WorldMap<K, W> {
    fn default() -> Self {
        Self {
            worlds: Default::default(),
            universe: std::ptr::null_mut(),
            parent_world: std::ptr::null_mut(),
            locked: false,
            delayed_insert: Vec::new(),
            delayed_remove: Vec::new()
        }
    }
}

impl<K: Eq + std::hash::Hash, W: WorldService> WorldMap<K, W> {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn len(&self) -> usize {
        self.worlds.len()
    }

    pub fn is_empty(&self) -> bool {
        self.worlds.is_empty()
    }

    pub fn update_universe_ptr(&mut self, universe: *mut Universe<W::UniverseServiceType>) {
        assert!(!self.locked);
        self.universe = universe;

        for (_, world) in self.worlds.iter_mut() {
            world.update_universe_ptr(universe);
        }
    }

    pub fn update_parent_world_ptr(&mut self, parent_world: *mut World<W::ParentWorldServiceType>) {
        assert!(!self.locked);

        self.parent_world = parent_world;

        for (_, world) in self.worlds.iter_mut() {
            world.update_parent_world_ptr(parent_world);
        }
    }

    pub fn get_or_default<F>(&mut self, key: K, default: F) -> &mut World<W>
    where
        F: FnOnce() -> World<W>
    {
        assert!(!self.universe.is_null());

        self.worlds.get_or_default(key, || {
            let mut world = default();
            world.update_universe_ptr(self.universe);
            world.update_parent_world_ptr(self.parent_world);
            world
        })
    }

    pub fn insert(&mut self, key: K, mut value: World<W>) {
        assert!(!self.universe.is_null());

        if self.locked {
            self.delayed_insert.push((key, value));
            return;
        }

        value.update_universe_ptr(self.universe);
        value.update_parent_world_ptr(self.parent_world);
        Self::try_drop_world(self.worlds.insert(key, value));
    }

    pub fn remove(&mut self, key: K) -> bool {
        if self.locked {
            if self.worlds.contains_key(&key) {
                self.delayed_remove.push(key);
                return true;
            }
            return false;
        }

        Self::try_drop_world(self.worlds.remove(&key))
    }

    pub fn tick(&mut self) {
        self.locked = true;
        for (_, world) in self.worlds.iter_mut() {
            world.tick();
        }
        self.locked = false;

        // Perform delayed removed
        for key in self.delayed_remove.drain(..) {
            Self::try_drop_world(self.worlds.remove(&key));
        }

        // Perform delayed inserts
        for (key, mut value) in self.delayed_insert.drain(..) {
            value.update_universe_ptr(self.universe);
            value.update_parent_world_ptr(self.parent_world);
            Self::try_drop_world(self.worlds.insert(key, value));
        }
    }

    fn try_drop_world(world: Option<World<W>>) -> bool {
        if let Some(mut world) = world {
            world.update_universe_ptr(std::ptr::null_mut());
            world.update_parent_world_ptr(std::ptr::null_mut());
            std::mem::drop(world);
            true
        } else {
            false
        }
    }
}