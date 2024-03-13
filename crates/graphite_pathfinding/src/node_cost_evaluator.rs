use std::cell::{Cell, OnceCell, RefCell};

use graphite_core_server::world::{World, WorldExtension};
use graphite_mc_constants::block::BlockAttributes;

pub trait NodeCostEvaluator {
    fn reset(&mut self);
    fn get_single_cost(&mut self, x: i32, y: i32, z: i32) -> f32;
    fn get_combined_cost(&mut self, x: i32, y: i32, z: i32, width: usize, height: usize) -> f32 {
        let mut worst = 0.0;

        for xo in 0..width as i32 {
            for yo in 0..height as i32 {
                for zo in 0..width as i32 {
                    let cost = self.get_single_cost(x+xo, y+yo, z+zo);
                    if cost == f32::INFINITY {
                        return f32::INFINITY;
                    }

                    if cost > worst {
                        worst = cost;
                    }
                }
            }   
        }

        worst
    }
}

pub struct WorldNodeCostEvaluator<'a, W: WorldExtension> {
    pub world: &'a World<W>
}

impl <'a, W: WorldExtension> NodeCostEvaluator for WorldNodeCostEvaluator<'a, W> {
    fn reset(&mut self) {
    }

    fn get_single_cost(&mut self, x: i32, y: i32, z: i32) -> f32 {
        let chunk = self.world.get_chunk(x >> 4, z >> 4);
        if let Some(chunk) = chunk {
            if let Some(block) = chunk.get_block(x, y, z) {
                let attr: &BlockAttributes = block.try_into().unwrap();

                if attr.is_pathfindable_land { // todo: support other types
                    return 0.0;
                } else {
                    return f32::INFINITY;
                }
            }
        }
        f32::INFINITY
    }
}

pub struct CachedNodeCostEvaluator<E: NodeCostEvaluator> {
    pub evaluator: E,
    pub single_cache: Box<[(i32, i32, i32, f32)]>,
    pub combined_cache: Box<[(i32, i32, i32, f32)]>,
}

impl <E: NodeCostEvaluator> NodeCostEvaluator for CachedNodeCostEvaluator<E> {
    fn reset(&mut self) {
        self.evaluator.reset();
        self.single_cache.fill((i32::MIN, i32::MIN, i32::MIN, 0.0_f32));
        self.combined_cache.fill((i32::MIN, i32::MIN, i32::MIN, 0.0_f32));
    }

    fn get_single_cost(&mut self, x: i32, y: i32, z: i32) -> f32 {
        let cache_index = ((x & 0xF) | ((y & 0xF) << 4) | ((z & 0xF) << 8)) as usize;
        let cached = self.single_cache[cache_index];
        if cached.0 == x && cached.1 == y && cached.2 == z {
            return cached.3;
        }

        let cost = self.evaluator.get_single_cost(x, y, z);
        self.single_cache[cache_index] = (x, y, z, cost);
        cost
    }

    fn get_combined_cost(&mut self, x: i32, y: i32, z: i32, width: usize, height: usize) -> f32 {
        let cache_index = ((x & 0xF) | ((y & 0xF) << 4) | ((z & 0xF) << 8)) as usize;
        let cached = self.combined_cache[cache_index];
        if cached.0 == x && cached.1 == y && cached.2 == z {
            return cached.3;
        }

        let mut worst = 0.0;

        for xo in 0..width as i32 {
            for yo in 0..height as i32 {
                for zo in 0..width as i32 {
                    let cost = self.get_single_cost(x+xo, y+yo, z+zo);
                    if cost == f32::INFINITY {
                        return f32::INFINITY;
                    }

                    if cost > worst {
                        worst = cost;
                    }
                }
            }   
        }

        self.combined_cache[cache_index] = (x, y, z, worst);
        worst
    }
}