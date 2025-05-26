use glam::{DVec3, Vec3Swizzles};

#[derive(Clone)]
pub struct Path {
    pub positions: Vec<(i32, i32, i32)>,
    pub cost: f32,
}

impl Path {
    pub fn get_next_ground_node(&mut self, entity_pos: DVec3, entity_width: usize) -> Option<DVec3> {
        let xz_offset = entity_width as f64 * 0.5;

        let mut next = self.positions.first();
        let mut next_pos = next.map(|next| DVec3::new(
            next.0 as f64 + xz_offset,
            next.1 as f64,
            next.2 as f64 + xz_offset
        ));

        while next_pos.is_some() {
            if self.positions.len() == 1 {
                let delta_y = next_pos.unwrap().y - entity_pos.y;
                if delta_y > 1.0 || delta_y < -3.0 {
                    break;
                }
                if next_pos.unwrap().xz().distance_squared(entity_pos.xz()) >= 0.1*0.1 {
                    break;
                }
            } else if next_pos.unwrap().xz().distance_squared(entity_pos.xz()) >= 0.5*0.5 {
                break;
            }
            
            self.positions.remove(0);
            next = self.positions.first();
            next_pos = next.map(|next| DVec3::new(
                next.0 as f64 + xz_offset,
                next.1 as f64,
                next.2 as f64 + xz_offset
            ));
        }

        next_pos
    }
}