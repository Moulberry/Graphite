use glam::DVec3;
use num::Zero;
use parry3d::{math::{Isometry, Vector}, na::Translation3, shape::{Cuboid, Shape}};

#[derive(Debug, Copy, Clone)]
pub struct AABB {
    min: DVec3,
    max: DVec3
}

impl AABB {
    pub const ZERO: AABB = AABB {
        min: DVec3::ZERO,
        max: DVec3::ZERO,
    };

    pub const fn centered_bottom_cube(size: f64) -> Self {
        Self::centered_bottom(size, size)
    }

    pub const fn centered_bottom(width: f64, height: f64) -> Self {
        Self {
            min: DVec3::new(-width/2.0, 0.0, -width/2.0),
            max: DVec3::new(width/2.0, height, width/2.0),
        }
    }

    pub fn new(min: DVec3, max: DVec3) -> AABB {
        if cfg!(debug_assertions) && (min.x > max.x || min.y > max.y || min.z > max.z) {
            panic!("min > max: min={}, max={}", min, max)
        } else {
            Self {
                min,
                max
            }
        }
    }

    pub fn try_new(min: DVec3, max: DVec3) -> Option<AABB> {
        if min.x > max.x || min.y > max.y || min.z > max.z {
            None
        } else {
            Some(Self {
                min,
                max
            })
        }
    }

    pub fn project_point_onto_box(self, point: DVec3) -> DVec3 {
        point.clamp(self.min, self.max)
    }

    pub fn distance_to_point(self, point: DVec3) -> f64 {
        self.project_point_onto_box(point).distance(point)
    }

    pub fn distance_sq_to_point(self, point: DVec3) -> f64 {
        self.project_point_onto_box(point).distance_squared(point)
    }

    pub fn contains(self, point: DVec3) -> bool {
        self.min.x <= point.x && point.x <= self.max.x &&
            self.min.y <= point.y && point.y <= self.max.y &&
            self.min.z <= point.z && point.z <= self.max.z
    }

    #[must_use]
    pub fn inflate(self, amount: f64) -> Option<AABB> {
        self.inflate_by_vec(DVec3::splat(amount))
    }

    #[must_use]
    pub fn inflate_by_vec(self, vec: DVec3) -> Option<AABB> {
        Self::try_new(
            self.min - vec,
            self.max + vec
        )
    }

    #[must_use]
    pub fn deflate(self, amount: f64) -> Option<AABB> {
        Self::try_new(
            self.min + DVec3::splat(amount),
            self.max - DVec3::splat(amount)
        )
    }

    #[must_use]
    pub fn expand(mut self, vec: DVec3) -> AABB {
        for i in 0..3 {
            if vec[i] < 0.0 {
                self.min[i] += vec[i];
            } else {
                self.max[i] += vec[i];
            }
        }
        self
    }

    #[must_use]
    pub fn translate(self, vec: DVec3) -> AABB {
        AABB {
            min: self.min + vec,
            max: self.max + vec,
        }
    }

    #[inline(always)]
    pub fn min(self) -> DVec3 {
        self.min
    }

    #[inline(always)]
    pub fn max(self) -> DVec3 {
        self.max
    }

    pub fn center(self) -> DVec3 {
        self.min * 0.5 + self.max * 0.5
    }

    #[must_use]
    pub fn minkowski_difference(self, other: &AABB) -> AABB {
        Self {
            min: DVec3::new(self.min.x - other.max.x, self.min.y - other.max.y, self.min.z - other.max.z),
            max: DVec3::new(self.max.x - other.min.x, self.max.y - other.min.y, self.max.z - other.min.z)
        }
    }

    pub fn intersects_aabb(self, other: AABB) -> bool {
        if ((self.min.x + self.max.x) - (other.min.x + other.max.x)).abs() > (self.max.x - self.min.x) + (other.max.x - other.min.x) {
            return false;
        }
        if ((self.min.y + self.max.y) - (other.min.y + other.max.y)).abs() > (self.max.y - self.min.y) + (other.max.y - other.min.y) {
            return false;
        }
        if ((self.min.z + self.max.z) - (other.min.z + other.max.z)).abs() > (self.max.z - self.min.z) + (other.max.z - other.min.z) {
            return false;
        }
        return true;
    }

    pub fn intersects(self, shape: &dyn Shape, isometry: &Isometry<f32>) -> bool {
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

    pub fn ray_box(self, normalized_ray: DVec3) -> Option<(f64, u8)> {
        let mut t_near = f64::MIN;
        let mut t_far = f64::MAX;
        let mut hit = 0;
    
        for i in 0..3 {
            if normalized_ray[i].is_zero() {
                if self.min()[i] >= 0.0 || self.max()[i] <= 0.0 {
                    return None;
                }
            } else {
                let inverse = 1.0 / normalized_ray[i];
    
                let near;
    
                if inverse >= 0.0 {
                    near = self.min()[i] * inverse;
                    t_far = t_far.min(self.max()[i] * inverse);
                } else {
                    near = self.max()[i] * inverse;
                    t_far = t_far.min(self.min()[i] * inverse);
                }
    
                if near > t_near {
                    t_near = near;
                    hit = 1 << i;
                } else if near == t_near {
                    hit |= 1 << i;
                }
    
                if t_near > t_far {
                    return None;
                }
            }
        }
    
        if t_near >= 0.0 {
            Some((t_near, hit))
        } else {
            None
        }
    }
}