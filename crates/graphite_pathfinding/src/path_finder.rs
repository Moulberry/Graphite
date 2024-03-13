use std::{collections::BinaryHeap, hash::BuildHasherDefault};

use indexmap::{map::Entry, IndexMap};
use rustc_hash::FxHasher;

use crate::{node_traverser::PathNodeTraverser, path::Path};

pub struct PathFinder<T: PathNodeTraverser> {
    pub traverser: T,
}

pub struct PathNode {
    parent_index: usize,
    cost: f32
}

struct SmallestCostHolder {
    index: usize,
    cost: f32,
    estimated_cost: f32,
}

impl PartialEq for SmallestCostHolder {
    fn eq(&self, other: &Self) -> bool {
        self.estimated_cost.eq(&other.estimated_cost) && self.cost.eq(&other.cost)
    }
}

impl Eq for SmallestCostHolder {}

impl PartialOrd for SmallestCostHolder {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SmallestCostHolder {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match other.estimated_cost.total_cmp(&self.estimated_cost) {
            std::cmp::Ordering::Equal => self.cost.total_cmp(&other.cost),
            ordering => ordering,
        }
    }
}

const MIN_IMPROVEMENT: f32 = 0.01;

impl <T: PathNodeTraverser> PathFinder<T> {
    pub fn new(traverser: T) -> Self {
        Self {
            traverser,
        }
    }

    pub fn find_exact_path(&mut self, start: (i32, i32, i32), end: (i32, i32, i32)) -> Option<Path> {
        self.find_path(start, end, false)
    }

    pub fn find_best_path(&mut self, start: (i32, i32, i32), end: (i32, i32, i32)) -> Path {
        self.find_path(start, end, true).unwrap()
    }

    fn reverse_path(mut pos: (i32, i32, i32), node: &PathNode,
            node_map: &IndexMap<(i32, i32, i32), PathNode, BuildHasherDefault<FxHasher>>) -> Path {
        let mut output = Vec::new();

        let mut node = node;
        let cost = node.cost;

        while node.parent_index != usize::MAX {
            output.insert(0, pos);
            
            let (parent_pos, parent_info) = node_map.get_index(node.parent_index).unwrap();

            pos = *parent_pos;
            node = parent_info;
        }

        Path {
            positions: output,
            cost
        }
    }

    fn find_path(&mut self, start: (i32, i32, i32), end: (i32, i32, i32), best_effort: bool) -> Option<Path> {
        self.traverser.reset();

        // todo: https://stackoverflow.com/questions/41297236/a-whats-the-best-data-structure-for-the-open-list

        // todo: use priority queue instead of binaryheap
        let mut binary_heap = BinaryHeap::new();
        binary_heap.push(SmallestCostHolder {
            index: 0,
            cost: 0.0,
            estimated_cost: f32::MAX,
        });

        // todo: try simply using an IndexMap
        let mut node_map: IndexMap<(i32, i32, i32), PathNode, BuildHasherDefault<FxHasher>> = IndexMap::default();
        node_map.insert(start, PathNode {
            parent_index: usize::MAX,
            cost: 0.0,
        });

        // todo: use heapless
        let mut successors = Vec::new();

        let mut count = 0;
        let mut best_node_index = 0;
        let mut best_node_cost = f32::MAX;

        while let Some(holder) = binary_heap.pop() {
            if best_effort {
                if holder.estimated_cost - holder.cost < best_node_cost {
                    best_node_cost = holder.estimated_cost - holder.cost;
                    best_node_index = holder.index;
                }
            }

            count += 1;
            if count > 512 {
                if best_effort {
                    let (node_pos, node_info) = node_map.get_index(best_node_index).unwrap();
                    return Some(Self::reverse_path(*node_pos, node_info, &node_map));
                } else {
                    return None;
                }
            }

            let (node_pos, node_info) = node_map.get_index(holder.index).unwrap();
            let node_pos = *node_pos;

            if self.traverser.is_done(node_pos, end) {
                return Some(Self::reverse_path(node_pos, node_info, &node_map));
            }

            let holder_index = holder.index;
            let holder_cost = holder.cost;

            if holder_cost > node_info.cost {
                // todo: is this called often?
                continue;
            }

            successors.clear();
            self.traverser.get_successors(node_pos.0, node_pos.1, node_pos.2, &mut successors);

            for successor in &successors {
                let new_cost = holder_cost + successor.move_cost;
                let new_index;

                // todo: can we cache indices of direct neighbors to avoid map lookup?

                match node_map.entry((successor.x, successor.y, successor.z)) {
                    Entry::Occupied(mut e) => {
                        if e.get().cost - new_cost > MIN_IMPROVEMENT {
                            new_index = e.index();
                            e.insert(PathNode {
                                parent_index: holder_index,
                                cost: new_cost,
                            });
                        } else {
                            continue;
                        }
                    },
                    Entry::Vacant(v) => {
                        new_index = v.index();
                        v.insert(PathNode {
                            parent_index: holder_index,
                            cost: new_cost,
                        });
                    },
                }

                let estimated_cost = new_cost + self.traverser.calculate_heuristic(
                    (successor.x, successor.y, successor.z), end);

                binary_heap.push(SmallestCostHolder {
                    index: new_index,
                    cost: new_cost,
                    estimated_cost,
                });
            }
        }

        if best_effort {
            let (node_pos, node_info) = node_map.get_index(best_node_index).unwrap();
            return Some(Self::reverse_path(*node_pos, node_info, &node_map));
        } else {
            return None;
        }
    }

    pub fn find_worst_path(&mut self, start: (i32, i32, i32), end: (i32, i32, i32), travel_distance: i32,
            needs_solid_surface: bool) -> Path {
        self.traverser.reset();

        // todo: https://stackoverflow.com/questions/41297236/a-whats-the-best-data-structure-for-the-open-list

        // todo: use priority queue instead of binaryheap
        let mut binary_heap = BinaryHeap::new();
        binary_heap.push(SmallestCostHolder {
            index: 0,
            cost: 0.0,
            estimated_cost: f32::MAX,
        });

        // todo: try simply using an IndexMap
        let mut node_map: IndexMap<(i32, i32, i32), PathNode, BuildHasherDefault<FxHasher>> = IndexMap::default();
        node_map.insert(start, PathNode {
            parent_index: usize::MAX,
            cost: 0.0,
        });

        // todo: use heapless
        let mut successors = Vec::new();

        let mut count = 0;
        let mut worst_node_index = 0;
        let mut worst_node_cost = f32::MAX;

        while let Some(holder) = binary_heap.pop() {
            if holder.estimated_cost - holder.cost < worst_node_cost {
                worst_node_cost = holder.estimated_cost - holder.cost;
                worst_node_index = holder.index;
            }

            count += 1;
            if count > 512 {
                let (node_pos, node_info) = node_map.get_index(worst_node_index).unwrap();
                    return Self::reverse_path(*node_pos, node_info, &node_map);
            }

            let (node_pos, node_info) = node_map.get_index(holder.index).unwrap();
            let node_pos = *node_pos;

            let dx = node_pos.0 - start.0;
            let dy = node_pos.1 - start.1;
            let dz = node_pos.2 - start.2;
            let distance_from_start_sq = dx*dx + dy*dy + dz*dz;

            if distance_from_start_sq >= travel_distance*travel_distance &&
                    (!needs_solid_surface || self.traverser.is_solid_surface(node_pos.0, node_pos.1, node_pos.2)) {
                return Self::reverse_path(node_pos, node_info, &node_map);
            }

            let holder_index = holder.index;
            let holder_cost = holder.cost;

            if holder_cost > node_info.cost {
                // todo: is this called often?
                continue;
            }

            successors.clear();
            self.traverser.get_successors(node_pos.0, node_pos.1, node_pos.2, &mut successors);

            for successor in &successors {
                let new_cost = holder_cost + successor.move_cost;
                let new_index;

                // todo: can we cache indices of direct neighbors to avoid map lookup?

                match node_map.entry((successor.x, successor.y, successor.z)) {
                    Entry::Occupied(mut e) => {
                        if e.get().cost - new_cost > MIN_IMPROVEMENT {
                            new_index = e.index();
                            e.insert(PathNode {
                                parent_index: holder_index,
                                cost: new_cost,
                            });
                        } else {
                            continue;
                        }
                    },
                    Entry::Vacant(v) => {
                        new_index = v.index();
                        v.insert(PathNode {
                            parent_index: holder_index,
                            cost: new_cost,
                        });
                    },
                }

                let estimated_cost = new_cost - self.traverser.calculate_heuristic(
                    (successor.x, successor.y, successor.z), end);

                binary_heap.push(SmallestCostHolder {
                    index: new_index,
                    cost: new_cost,
                    estimated_cost,
                });
            }
        }

        let (node_pos, node_info) = node_map.get_index(worst_node_index).unwrap();
        return Self::reverse_path(*node_pos, node_info, &node_map);
    }
}