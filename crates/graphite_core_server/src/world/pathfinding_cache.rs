use std::cell::{RefCell, RefMut};

#[derive(Copy, Clone)]
pub struct PathfindingCacheEntry {
    pub region_x: i8,
    pub region_y: i8,
    pub region_z: i8,
    pub flags: u8,
    pub cost: f32,
}

impl PathfindingCacheEntry {
    #[inline(always)]
    pub fn invalid() -> Self {
        Self {
            region_x: 0, region_y: 0, region_z: 0, cost: 0.0, flags: 0
        }
    }
}

pub struct PathfindingCacheHolder {
    cache: RefCell<PathfindingCache>,
}

pub struct PathfindingCache {
    pub single: Box<[PathfindingCacheEntry]>,
    pub combined: Box<[PathfindingCacheEntry]>,
    invalid: bool,
    last_combined_width: usize,
    last_combined_height: usize
}

impl PathfindingCacheHolder {
    pub fn new() -> Self {
        Self {
            cache: RefCell::new(PathfindingCache {
                single: Box::new([]),
                combined: Box::new([]),
                invalid: true,
                last_combined_width: usize::MAX,
                last_combined_height: usize::MAX,
            }),
        }
    }

    pub fn invalidate(&mut self) {
        let cache_mut = self.cache.get_mut();

        if !cache_mut.invalid {
            cache_mut.single.fill(PathfindingCacheEntry::invalid());
            cache_mut.combined.fill(PathfindingCacheEntry::invalid());
            cache_mut.invalid = true;
        }
    }

    pub fn get(&self, width: usize, height: usize) -> RefMut<PathfindingCache> {
        let mut cache_mut = self.cache.borrow_mut();

        // Allocate if needed
        if cache_mut.single.is_empty() {
            cache_mut.single = vec![PathfindingCacheEntry::invalid(); 16*16*16].into_boxed_slice();
        }
        if cache_mut.combined.is_empty() {
            cache_mut.combined = vec![PathfindingCacheEntry::invalid(); 16*16*16].into_boxed_slice();
        }

        // Invalidate combined cache if width/height changes
        if width != cache_mut.last_combined_width || height != cache_mut.last_combined_height {
            cache_mut.last_combined_width = width;
            cache_mut.last_combined_height = height;
            if !cache_mut.invalid {
                cache_mut.combined.fill(PathfindingCacheEntry::invalid());
            }
        }

        cache_mut.invalid = false;
        cache_mut
    }
}