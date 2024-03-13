use glam::{DVec2, DVec3, Vec3Swizzles};

pub struct Path {
    pub positions: Vec<(i32, i32, i32)>,
    pub cost: f32,
}

impl Path {
    pub fn get_next_ground_node(&mut self, entity_pos: DVec3, entity_width: usize) -> Option<DVec2> {
        let xz_offset = entity_width as f64 * 0.5;

        let mut next = self.positions.first();
        let mut next_pos = next.map(|next| DVec2::new(
            next.0 as f64 + xz_offset,
            next.2 as f64 + xz_offset
        ));

        while next_pos.is_some() {
            let required_distance_sq = if self.positions.len() == 1 {
                0.1*0.1
            } else {
                0.5*0.5
            };

            if next_pos.unwrap().distance_squared(entity_pos.xz()) >= required_distance_sq {
                break;
            }
            self.positions.remove(0);
            next = self.positions.first();
            next_pos = next.map(|next| DVec2::new(
                next.0 as f64 + xz_offset,
                next.2 as f64 + xz_offset
            ));
        }

        next_pos
    }
}