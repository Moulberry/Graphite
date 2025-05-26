
pub struct EntityIterator<'a> {
    entities: slab::Iter<'a, hecs::Entity>,
    world: &'a hecs::World
}

impl <'a> EntityIterator<'a> {
    pub fn new(entities: slab::Iter<'a, hecs::Entity>, world: &'a hecs::World) -> Self {
        Self {
            entities,
            world
        }
    }
}

impl <'a> Iterator for EntityIterator<'a> {
    type Item = hecs::EntityRef<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some((_, entity)) = self.entities.next() {
                return Some(self.world.entity(*entity).unwrap());
            } else {
                return None;
            }
        }
    }
}