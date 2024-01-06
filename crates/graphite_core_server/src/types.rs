use glam::DVec3;

#[derive(Debug, Copy, Clone)]
pub struct AABB {
    min: DVec3,
    max: DVec3
}

impl AABB {
    pub fn new(min: DVec3, max: DVec3) -> Option<AABB> {
        if min.x <= max.x && min.y <= max.y && min.z <= max.z {
            Some(Self {
                min,
                max
            })
        } else {
            None
        }
    }

    pub fn expand(&self, vec: DVec3) -> AABB {
        let mut bounds = *self;
        for i in 0..3 {
            if vec[i] < 0.0 {
                bounds.min[i] += vec[i];
            } else {
                bounds.max[i] += vec[i];
            }
        }
        bounds
    }

    pub fn min(&self) -> DVec3 {
        self.min
    }

    pub fn max(&self) -> DVec3 {
        self.max
    }

    pub fn minkowski_difference(&self, other: AABB) -> AABB {
        let min = DVec3::new(self.min.x - other.max.x, self.min.y - other.max.y, self.min.z - other.max.z);
        let max = DVec3::new(self.max.x - other.min.x, self.max.y - other.min.y, self.max.z - other.min.z);

        Self::new(min, max).unwrap()
    }
}