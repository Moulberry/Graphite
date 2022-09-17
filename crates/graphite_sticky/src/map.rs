use hashbrown::HashMap;

use crate::Unsticky;

#[derive(Debug)]
pub struct StickyMap<K, V: Unsticky> {
    inner: HashMap<K, Box<V>>
}

impl<K, V: Unsticky> Default for StickyMap<K, V> {
    fn default() -> Self {
        Self { inner: Default::default() }
    }
}

impl<K: Eq + std::hash::Hash, V: Unsticky> StickyMap<K, V> {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    pub fn get_or_default<F>(&mut self, key: K, default: F) -> &mut V
    where
        F: FnOnce() -> V
    {
        self.inner.entry(key).or_insert_with(|| {
            let mut boxed = Box::from(default());
            boxed.update_pointer();
            boxed
        })
    }

    pub fn contains_key(&self, key: &K) -> bool {
        self.inner.contains_key(key)
    }

    pub fn insert(&mut self, key: K, value: V) -> Option<V::UnstuckType> {
        let mut boxed = Box::from(value);
        boxed.update_pointer();
        self.inner.insert(key, boxed).map(|v| v.unstick())
    }

    pub fn remove(&mut self, key: &K) -> Option<V::UnstuckType> {
        self.inner.remove(key).map(|v| v.unstick())
    }

    pub fn drain_filter<F>(&mut self, pred: F) -> DrainFilter<'_, K, V, F>
    where
        F: FnMut(&K, &mut Box<V>) -> bool,
    {
        DrainFilter {
            base: self.inner.drain_filter(pred)
        }
    }

    pub fn iter(&mut self) -> Iter<'_, K, V> {
        Iter {
            base: self.inner.iter()
        }
    }

    pub fn iter_mut(&mut self) -> IterMut<'_, K, V> {
        IterMut {
            base: self.inner.iter_mut()
        }
    }
}

pub struct Iter<'a, K, V> {
    base: hashbrown::hash_map::Iter<'a, K, Box<V>>
}

impl<'a, K, V> Iterator for Iter<'a, K, V> {
    type Item = (&'a K, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        self.base.next().map(|(k, v)| (k, v.as_ref()))
    }
}

pub struct IterMut<'a, K, V> {
    base: hashbrown::hash_map::IterMut<'a, K, Box<V>>
}

impl<'a, K, V> Iterator for IterMut<'a, K, V> {
    type Item = (&'a K, &'a mut V);

    fn next(&mut self) -> Option<Self::Item> {
        self.base.next().map(|(k, v)| (k, v.as_mut()))
    }
}

pub struct DrainFilter<'a, K, V, F>
where
    V: Unsticky,
    F: FnMut(&K, &mut Box<V>) -> bool,
{
    base: hashbrown::hash_map::DrainFilter<'a, K, Box<V>, F>,
}

impl<'a, K, V, F> Iterator for DrainFilter<'a, K, V, F>
where
    V: Unsticky,
    F: FnMut(&K, &mut Box<V>) -> bool,
{
    type Item = (K, V::UnstuckType);

    fn next(&mut self) -> Option<Self::Item> {
        self.base.next().map(|(k, v)| (k, v.unstick()))
    }
}
