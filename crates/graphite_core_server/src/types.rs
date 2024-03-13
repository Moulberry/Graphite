use glam::DVec3;
use parry3d::{math::{Isometry, Vector}, na::Translation3, shape::{Cuboid, Shape}};

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

    pub fn intersects(&self, shape: &dyn Shape, isometry: &Isometry<f32>) -> bool {
        let self_shape = Cuboid::new(Vector::new(
            (self.max.x - self.min.x) as f32 / 2.0,
            (self.max.y - self.min.y) as f32 / 2.0,
            (self.max.z - self.min.z) as f32 / 2.0,
        ));
        let mut self_isometry = Isometry::identity();
        self_isometry.append_translation_mut(&Translation3::new(
            (self.max.x + self.min.x) as f32 / 2.0,
            (self.max.y + self.min.y) as f32 / 2.0,
            (self.max.z + self.min.z) as f32 / 2.0,
        ));

        parry3d::query::intersection_test(&self_isometry,
            &self_shape, isometry, shape).unwrap()
    }
}