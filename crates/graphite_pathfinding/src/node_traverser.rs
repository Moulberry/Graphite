use crate::node_cost_evaluator::NodeCostEvaluator;

pub struct Successor {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub move_cost: f32
}

pub trait PathNodeTraverser {
    fn reset(&mut self);
    fn is_solid_surface(&mut self, x: i32, y: i32, z: i32) -> bool;
    fn get_successors(&mut self, x: i32, y: i32, z: i32, successors: &mut Vec<Successor>);
    fn calculate_heuristic(&self, node: (i32, i32, i32), target: (i32, i32, i32)) -> f32;
    fn is_done(&self, node: (i32, i32, i32), target: (i32, i32, i32)) -> bool;
}

pub struct LandPathNodeTraverser<E: NodeCostEvaluator> {
    pub evaluator: E,
    pub entity_width: usize,
    pub entity_height: usize,
}

impl <E: NodeCostEvaluator> LandPathNodeTraverser<E> {
    fn get_successor(&mut self, x: i32, y: i32, z: i32, dist: f32, max_steps: i32) -> Successor {
        let mut cost = self.evaluator.get_combined_cost(x, y, z, self.entity_width, self.entity_height);

        if cost == f32::INFINITY {
            let mut steps = 0;
            while cost == f32::INFINITY && steps < max_steps {
                steps += 1;
                cost = self.evaluator.get_combined_cost(x, y+steps, z, self.entity_width, self.entity_height);
            }

            Successor {
                x,
                y: y+steps,
                z,
                move_cost: cost + dist + steps as f32 * 0.5
            }
        } else {
            let mut steps = 0;
            loop {
                if steps > 32 {
                    return Successor {
                        x, y, z,
                        move_cost: f32::INFINITY
                    }; 
                }

                let below_cost = self.evaluator.get_combined_cost(x, y-steps-1, z, self.entity_width, self.entity_height);
                if below_cost == f32::INFINITY {
                    if steps <= 3 {
                        cost += steps as f32 * 0.5;
                    } else {
                        cost += (steps as f32 - 3.0) * 4.0;
                    }
                    
                    return Successor {
                        x,
                        y: y-steps,
                        z,
                        move_cost: cost + dist
                    };
                }

                cost += below_cost;
                steps += 1;
            }
        }
    }

    fn valid_successor(&mut self, successor: &Successor) -> bool {
        successor.move_cost != f32::INFINITY
    }

    fn valid_diagonal(&mut self, from_y: i32, neighbor1: &Successor, neighbor2: &Successor, diagonal: &Successor) -> bool {
        if !self.valid_successor(diagonal) {
            return false;
        }

        if (neighbor1.y > from_y || neighbor2.y > from_y) && diagonal.y <= from_y {
            return false;
        }

        return (neighbor1.y < from_y || self.valid_successor(neighbor1)) &&
            (neighbor2.y < from_y || self.valid_successor(neighbor2));
    }
}

const SQRT_2: f32 = 1.4142135623730951;

impl <E: NodeCostEvaluator> PathNodeTraverser for LandPathNodeTraverser<E> {
    fn reset(&mut self) {
        self.evaluator.reset();
    }

    fn is_solid_surface(&mut self, x: i32, y: i32, z: i32) -> bool {
        for xo in 0..self.entity_width as i32 {
            for zo in 0..self.entity_width as i32 {
                let cost = self.evaluator.get_single_cost(x+xo, y-1, z+zo);
                if cost != f32::INFINITY {
                    return false;
                }
            }
        }
        true
    }

    // todo: maybe StackVec?
    fn get_successors(&mut self, x: i32, y: i32, z: i32, successors: &mut Vec<Successor>) {
        let above_cost = self.evaluator.get_combined_cost(x, y+1, z, self.entity_width, self.entity_height);
        let max_steps = if above_cost == f32::INFINITY {
            0
        } else {
            1
        };

        let north = self.get_successor(x, y, z-1, 1.0, max_steps);
        let south = self.get_successor(x, y, z+1, 1.0, max_steps);
        let east = self.get_successor(x+1, y, z, 1.0, max_steps);
        let west = self.get_successor(x-1, y, z, 1.0, max_steps);

        let north_east = self.get_successor(x+1, y, z-1, SQRT_2, max_steps);
        if self.valid_diagonal(y, &north, &east, &north_east) {
            successors.push(north_east);
        }

        let north_west = self.get_successor(x-1, y, z-1, SQRT_2, max_steps);
        if self.valid_diagonal(y, &north, &west, &north_west) {
            successors.push(north_west);
        }

        let south_east = self.get_successor(x+1, y, z+1, SQRT_2, max_steps);
        if self.valid_diagonal(y, &south, &east, &south_east) {
            successors.push(south_east);
        }

        let south_west = self.get_successor(x-1, y, z+1, SQRT_2, max_steps);
        if self.valid_diagonal(y, &south, &west, &south_west) {
            successors.push(south_west);
        }

        if self.valid_successor(&north) {
            successors.push(north);
        }

        if self.valid_successor(&south) {
            successors.push(south);
        }

        if self.valid_successor(&east) {
            successors.push(east);
        }

        if self.valid_successor(&west) {
            successors.push(west);
        }
    }

    fn calculate_heuristic(&self, node: (i32, i32, i32), target: (i32, i32, i32)) -> f32 {
        let mut dx = node.0 - target.0;
        let mut dy = node.1 - target.1;
        let mut dz = node.2 - target.2;

        if dx > 0 {
            dx = (dx - self.entity_width as i32 + 1).max(0);
        }
        if dy > 0 {
            dy = (dy - self.entity_height as i32 + 1).max(0);
        }
        if dz > 0 {
            dz = (dz - self.entity_width as i32 + 1).max(0);
        }

        dx = dx.abs();
        dy = dy.abs();
        dz = dz.abs();

        let min = dx.min(dz) as f32;

        let diagonal = SQRT_2 * min;
        let direct = (dx + dz) as f32 - min * 2.0;
        let vertical = dy as f32 * 0.5;

        return diagonal + direct + vertical;
    }

    fn is_done(&self, node: (i32, i32, i32), target: (i32, i32, i32)) -> bool {
        target.0 >= node.0 && target.0 < node.0 + self.entity_width as i32 &&
            target.1 >= node.1 && target.1 < node.1 + self.entity_height as i32 &&
            target.2 >= node.2 && target.2 < node.2 + self.entity_width as i32
    }
}

