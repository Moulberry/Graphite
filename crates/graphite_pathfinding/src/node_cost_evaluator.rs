use std::cell::RefMut;

use graphite_core_server::world::{chunk::ChunkProvider, pathfinding_cache::{PathfindingCache, PathfindingCacheEntry}, World, WorldExtension};
use graphite_mc_constants::block::{BlockAttributes, BlockFlag, BlockState};

pub struct Cost {
    pub value: f32,
    pub is_swimmable: bool
}

pub trait NodeCostEvaluator {
    fn get_single_cost(&mut self, x: i32, y: i32, z: i32) -> Cost;
    fn get_combined_cost(&mut self, x: i32, y: i32, z: i32, width: usize, height: usize) -> Cost {
        let mut worst = Cost {
            value: 0.0,
            is_swimmable: false
        };

        for yo in 0..height as i32 {
            for xo in 0..width as i32 {
                for zo in 0..width as i32 {
                    let cost = self.get_single_cost(x+xo, y+yo, z+zo);
                    if cost.value == f32::INFINITY {
                        return cost;
                    }

                    if cost.value > worst.value {
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
    fn get_single_cost(&mut self, x: i32, y: i32, z: i32) -> Cost {
        let chunk = self.world.get_chunk(x >> 4, z >> 4);
        if let Some(chunk) = chunk {
            const HONEY: u16 = BlockState::HoneyBlock {  }.to_id();
            if let Some(block) = chunk.get_block(x, y, z) {
                if block == HONEY {
                    return Cost {
                        value: f32::INFINITY,
                        is_swimmable: false
                    }
                }

                let attr = BlockAttributes::from_block_state(block);

                if attr.has_flag(BlockFlag::IsPathfindableLand) { // todo: support other types
                    if attr.has_flag(BlockFlag::Waterlogged) {
                        return Cost {
                            value: 4.0,
                            is_swimmable: true
                        }
                    } else {
                        return Cost {
                            value: 0.0,
                            is_swimmable: false
                        }
                    }
                } else {
                    return Cost {
                        value: f32::INFINITY,
                        is_swimmable: false
                    }
                }
            }
        }

        return Cost {
            value: f32::INFINITY,
            is_swimmable: false
        }
    }
}

pub struct CachedNodeCostEvaluator<'a, E: NodeCostEvaluator> {
    pub evaluator: E,
    pub cache: RefMut<'a, PathfindingCache>,
}

impl <'a, E: NodeCostEvaluator> NodeCostEvaluator for CachedNodeCostEvaluator<'a, E> {
    fn get_single_cost(&mut self, x: i32, y: i32, z: i32) -> Cost {
        let cache_index = ((x & 15) | ((y & 15) << 4) | ((z & 15) << 8)) as usize;
        let cached = self.cache.single[cache_index];

        let region_x = (x >> 4) as i8;
        let region_y = (y >> 4) as i8;
        let region_z = (z >> 4) as i8;

        if (cached.flags & 1) != 0 && cached.region_x == region_x && cached.region_y == region_y && cached.region_z == region_z {
            return Cost {
                value: cached.cost,
                is_swimmable: (cached.flags & 2) != 0
            }
        }

        let cost = self.evaluator.get_single_cost(x, y, z);
        let mut flags = 1;
        if cost.is_swimmable {
            flags |= 2;
        }
        self.cache.single[cache_index] = PathfindingCacheEntry {
            region_x,
            region_y,
            region_z,
            flags,
            cost: cost.value,
        };
        cost
    }

    fn get_combined_cost(&mut self, x: i32, y: i32, z: i32, width: usize, height: usize) -> Cost {
        let cache_index = ((x & 15) | ((y & 15) << 4) | ((z & 15) << 8)) as usize;
        let cached = self.cache.combined[cache_index];
        
        let region_x = (x >> 4) as i8;
        let region_y = (y >> 4) as i8;
        let region_z = (z >> 4) as i8;

        if (cached.flags & 1) != 0 && cached.region_x == region_x && cached.region_y == region_y && cached.region_z == region_z {
            return Cost {
                value: cached.cost,
                is_swimmable: (cached.flags & 2) != 0
            }
        }

        let mut worst = Cost {
            value: 0.0,
            is_swimmable: false
        };

        for yo in 0..height as i32 {
            for xo in 0..width as i32 {
                for zo in 0..width as i32 {
                    let cost = self.get_single_cost(x+xo, y+yo, z+zo);
                    if cost.value == f32::INFINITY {
                        return cost;
                    }

                    if cost.value > worst.value {
                        worst = cost;
                    }
                }
            }   
        }

        let mut flags = 1;
        if worst.is_swimmable {
            flags |= 2;
        }
        self.cache.combined[cache_index] = PathfindingCacheEntry {
            region_x,
            region_y,
            region_z,
            flags,
            cost: worst.value,
        };
        worst
    }
}