use glam::{Mat3, Mat4, Quat, Vec3};
use graphite_binary::nbt::{CompoundRef, NBT, TAG_FLOAT_ID};

#[derive(PartialEq, Clone, Debug)]
pub struct Transform {
    pub translation: (f32, f32, f32),
    pub left_rotation: (f32, f32, f32, f32),
    pub scale: (f32, f32, f32),
    pub right_rotation: (f32, f32, f32, f32),
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            translation: (0.0, 0.0, 0.0),
            left_rotation: (0.0, 0.0, 0.0, 1.0),
            scale: (1.0, 1.0, 1.0),
            right_rotation: (0.0, 0.0, 0.0, 1.0),
        }
    }
}

impl Transform {
    pub fn scale(x: f32, y: f32, z: f32) -> Self {
        Self {
            scale: (x, y, z),
            ..Default::default()
        }
    }

    pub fn translation(x: f32, y: f32, z: f32) -> Self {
        Self {
            translation: (x, y, z),
            ..Default::default()
        }
    }

    pub fn load_from_entity(entity: CompoundRef<'_>) -> Self {
        Self::try_load_from_entity(entity).unwrap_or(Default::default())
    }

    pub fn write_to_entity(&self, entity: &mut NBT) {
        let Some(mut entity) = entity.as_compound_mut() else {
            return;
        };

        let mut transformation = if let Some(transformation) = entity.find_compound_mut("transformation") {
            transformation
        } else {
            entity.create_compound("transformation")
        };

        let mut translation = if let Some(translation) = transformation.find_list_mut("translation", TAG_FLOAT_ID) {
            translation
        } else {
            transformation.create_list("translation", TAG_FLOAT_ID)
        };

        translation.set_float_at(0, self.translation.0);
        translation.set_float_at(1, self.translation.1);
        translation.set_float_at(2, self.translation.2);

        let mut left_rotation = if let Some(left_rotation) = transformation.find_list_mut("left_rotation", TAG_FLOAT_ID) {
            left_rotation
        } else {
            transformation.create_list("left_rotation", TAG_FLOAT_ID)
        };

        left_rotation.set_float_at(0, self.left_rotation.0);
        left_rotation.set_float_at(1, self.left_rotation.1);
        left_rotation.set_float_at(2, self.left_rotation.2);
        left_rotation.set_float_at(3, self.left_rotation.3);

        let mut scale = if let Some(scale) = transformation.find_list_mut("scale", TAG_FLOAT_ID) {
            scale
        } else {
            transformation.create_list("scale", TAG_FLOAT_ID)
        };

        scale.set_float_at(0, self.scale.0);
        scale.set_float_at(1, self.scale.1);
        scale.set_float_at(2, self.scale.2);

        let mut right_rotation = if let Some(right_rotation) = transformation.find_list_mut("right_rotation", TAG_FLOAT_ID) {
            right_rotation
        } else {
            transformation.create_list("right_rotation", TAG_FLOAT_ID)
        };

        right_rotation.set_float_at(0, self.right_rotation.0);
        right_rotation.set_float_at(1, self.right_rotation.1);
        right_rotation.set_float_at(2, self.right_rotation.2);
        right_rotation.set_float_at(3, self.right_rotation.3);
    }

    pub fn try_load_from_entity(entity: CompoundRef<'_>) -> Option<Self> {
        if let Some(transformation) = entity.find_compound("transformation") {
            let translation = if let Some(translation) = transformation.find_list("translation", TAG_FLOAT_ID) {
                (
                    *translation.get_float(0).unwrap(),
                    *translation.get_float(1).unwrap(),
                    *translation.get_float(2).unwrap(),
                )
            } else {
                (0.0, 0.0, 0.0)
            };
            let left_rotation = if let Some(left_rotation) = transformation.find_list("left_rotation", TAG_FLOAT_ID) {
                (
                    *left_rotation.get_float(0).unwrap(),
                    *left_rotation.get_float(1).unwrap(),
                    *left_rotation.get_float(2).unwrap(),
                    *left_rotation.get_float(3).unwrap(),
                )
            } else {
                (0.0, 0.0, 0.0, 1.0)
            };
            let scale = if let Some(scale) = transformation.find_list("scale", TAG_FLOAT_ID) {
                (
                    *scale.get_float(0).unwrap(),
                    *scale.get_float(1).unwrap(),
                    *scale.get_float(2).unwrap(),
                )
            } else {
                (1.0, 1.0, 1.0)
            };
            let right_rotation = if let Some(right_rotation) = transformation.find_list("right_rotation", TAG_FLOAT_ID) {
                (
                    *right_rotation.get_float(0).unwrap(),
                    *right_rotation.get_float(1).unwrap(),
                    *right_rotation.get_float(2).unwrap(),
                    *right_rotation.get_float(3).unwrap(),
                )
            } else {
                (0.0, 0.0, 0.0, 1.0)
            };

            Some(Self {
                translation,
                left_rotation,
                scale,
                right_rotation,
            })
        } else {
            return None;
        }
    }

    pub fn to_matrix(&self) -> glam::Mat4 {
        let mut matrix = Mat4::IDENTITY;

        matrix *= Mat4::from_translation(Vec3::from(self.translation));
        matrix *= Mat4::from_quat(Quat::from_xyzw(self.left_rotation.0, self.left_rotation.1,
            self.left_rotation.2, self.left_rotation.3));
        matrix *= Mat4::from_scale(Vec3::from(self.scale));
        matrix *= Mat4::from_quat(Quat::from_xyzw(self.right_rotation.0, self.right_rotation.1,
            self.right_rotation.2, self.right_rotation.3));

        matrix
        
    }

    pub fn from_matrix(mat4: glam::Mat4) -> Self {
        let mult = 1.0 / mat4.w_axis.w;
        let mat3 = Mat3::from_mat4(mat4).mul_scalar(mult);

        let (left, scale, right) = svd_decompose(mat3);

        Self {
            translation: (mat4.w_axis.x * mult, mat4.w_axis.y * mult, mat4.w_axis.z * mult),
            left_rotation: (left.x, left.y, left.z, left.w),
            scale: (scale.x, scale.y, scale.z),
            right_rotation: (right.x, right.y, right.z, right.w),
        }
    }

    pub fn to_rotation(&self) -> glam::Quat {
        let left = self.left_rotation;
        let left = Quat::from_xyzw(left.0, left.1, left.2, left.3);
        let right = self.right_rotation;
        let right = Quat::from_xyzw(right.0, right.1, right.2, right.3);
        left.mul_quat(right)
    }
}

fn svd_decompose(matrix3f: Mat3) -> (Quat, Vec3, Quat) {
    let mut matrix3f2 = matrix3f.clone();
    matrix3f2 = matrix3f2.transpose();
    matrix3f2 = matrix3f2 * matrix3f;
    let quaternionf = eigenvalue_jacobi(&mut matrix3f2, 5);
    let f = matrix3f2.x_axis.x;
    let g = matrix3f2.y_axis.y;
    let bl = f < 1.0E-6;
    let bl2 = g < 1.0E-6;
    let mut matrix3f3 = matrix3f2;
    let matrix3f4 = matrix3f.mul_mat3(&Mat3::from_quat(quaternionf));
    let mut quaternionf2 = Quat::IDENTITY;
    let mut givens_parameters = if bl {
        qr_givens_quat(matrix3f4.y_axis.y, -matrix3f4.y_axis.x)
    } else {
        qr_givens_quat(matrix3f4.x_axis.x, matrix3f4.x_axis.y)
    };
    let quaternionf4 = givens_parameters.around_z();
    let mut matrix3f5 = givens_parameters.matrix_around_z(&mut matrix3f3);
    quaternionf2 = quaternionf2 * quaternionf4;
    matrix3f5 = matrix3f5.transpose() * matrix3f4;
    matrix3f3 = matrix3f4;
    givens_parameters = if bl {
        qr_givens_quat(matrix3f5.z_axis.z, -matrix3f5.z_axis.x)
    } else {
        qr_givens_quat(matrix3f5.x_axis.x, matrix3f5.x_axis.z)
    };
    givens_parameters = givens_parameters.inverse();
    let quaternionf5 = givens_parameters.around_y();
    let mut matrix3f6 = givens_parameters.matrix_around_y(&mut matrix3f3);
    quaternionf2 = quaternionf2 * quaternionf5;
    matrix3f6 = matrix3f6.transpose() * matrix3f5;
    matrix3f3 = matrix3f5;
    givens_parameters = if bl2 {
        qr_givens_quat(matrix3f6.z_axis.z, -matrix3f6.z_axis.y)
    } else {
        qr_givens_quat(matrix3f6.y_axis.y, matrix3f6.y_axis.z)
    } ;
    let quaternionf6 = givens_parameters.around_x();
    let mut matrix3f7 = givens_parameters.matrix_around_x(&mut matrix3f3);
    quaternionf2 = quaternionf2 * quaternionf6;
    matrix3f7 = matrix3f7.transpose() * matrix3f6;

    (
        quaternionf2,
        Vec3::new(matrix3f7.x_axis.x, matrix3f7.y_axis.y, matrix3f7.z_axis.z),
        quaternionf.conjugate()
    )
}

const G: f32 = 5.82842712474619;
const PI_4: GivensParameters = GivensParameters {
    sin_half: 0.3826834492732639,
    cos_half: 0.9238795255076916,
};
fn approx_givens_quat(f: f32, g: f32, h: f32) -> GivensParameters {
    let j: f32 = g;
    let i: f32 = 2.0 * (f - h);
    if G * j * j < i * i {
        return GivensParameters::from_unnormalized(j, i);
    }
    return PI_4;
}

fn qr_givens_quat(f: f32, g: f32) -> GivensParameters {
    let h: f32 = f.hypot(g);
    let mut i: f32 = if h > 1.0E-6 {
        g
    } else {
        0.0
    };
    let mut j: f32 = f.abs() + h.max(1.0E-6);
    if f < 0.0 {
        let k: f32 = i;
        i = j;
        j = k;
    }
    return GivensParameters::from_unnormalized(i, j);
}

fn similarity_transform(matrix3f: &mut Mat3, matrix3f2: &mut Mat3) {
    *matrix3f = *matrix3f * *matrix3f2;
    *matrix3f2 = matrix3f2.transpose();
    *matrix3f2 = *matrix3f2 * *matrix3f;
    *matrix3f = *matrix3f2;
}

fn step_jacobi(matrix3f: &mut Mat3, matrix3f2: &mut Mat3, quaternionf2: &mut Quat) {
    let mut givens_parameters;
    if matrix3f.x_axis.y * matrix3f.x_axis.y + matrix3f.y_axis.x * matrix3f.y_axis.x > 1.0E-6 {
        givens_parameters = approx_givens_quat(matrix3f.x_axis.x, 0.5 * (matrix3f.x_axis.y + matrix3f.y_axis.x), matrix3f.y_axis.y);
        *quaternionf2 = *quaternionf2 * givens_parameters.around_z();
        *matrix3f2 = givens_parameters.matrix_around_z(matrix3f2);
        similarity_transform(matrix3f, matrix3f2);
    }
    if matrix3f.x_axis.z * matrix3f.x_axis.z + matrix3f.z_axis.x * matrix3f.z_axis.x > 1.0E-6 {
        givens_parameters = approx_givens_quat(matrix3f.x_axis.x, 0.5 * (matrix3f.x_axis.z + matrix3f.z_axis.x), matrix3f.z_axis.z).inverse();
        *quaternionf2 = *quaternionf2 * givens_parameters.around_y();
        *matrix3f2 = givens_parameters.matrix_around_y(matrix3f2);
        similarity_transform(matrix3f, matrix3f2);
    }
    if matrix3f.y_axis.z * matrix3f.y_axis.z + matrix3f.z_axis.y * matrix3f.z_axis.y > 1.0E-6 {
        givens_parameters = approx_givens_quat(matrix3f.y_axis.y, 0.5 * (matrix3f.y_axis.z + matrix3f.z_axis.y), matrix3f.z_axis.z);
        *quaternionf2 = *quaternionf2 * givens_parameters.around_x();
        *matrix3f2 = givens_parameters.matrix_around_x(matrix3f2);
        similarity_transform(matrix3f, matrix3f2);
    }
}

fn eigenvalue_jacobi(matrix3f: &mut Mat3, count: usize) -> Quat {
    let mut quaternionf = Quat::IDENTITY;
    let mut matrix3f2 = Mat3::IDENTITY;
    for _ in 0..count {
        step_jacobi(matrix3f, &mut matrix3f2, &mut quaternionf);
    }
    quaternionf = quaternionf.normalize();
    return quaternionf;
}

struct GivensParameters {
    sin_half: f32,
    cos_half: f32
}

impl GivensParameters {
    fn new(sin_half: f32, cos_half: f32) -> Self {
        Self {
            sin_half,
            cos_half
        }
    }

    fn from_unnormalized(f: f32, g: f32) -> GivensParameters {
        let h: f32 = 1.0 / (f * f + g * g).sqrt();
        return GivensParameters::new(h * f, h * g);
    }

    fn inverse(&self) -> GivensParameters {
        return GivensParameters::new(-self.sin_half, self.cos_half);
    }

    fn around_x(&self) -> Quat {
        return Quat::from_xyzw(self.sin_half, 0.0, 0.0, self.cos_half);
    }

    fn around_y(&self) -> Quat {
        return Quat::from_xyzw(0.0, self.sin_half, 0.0, self.cos_half);
    }

    fn around_z(&self,) -> Quat {
        return Quat::from_xyzw(0.0, 0.0, self.sin_half, self.cos_half);
    }

    fn cos(&self) -> f32 {
        return self.cos_half * self.cos_half - self.sin_half * self.sin_half;
    }

    fn sin(&self) -> f32 {
        return 2.0 * self.sin_half * self.cos_half;
    }

    fn matrix_around_x(&self, matrix3f: &mut Mat3) -> Mat3 {
        matrix3f.x_axis.y = 0.0;
        matrix3f.x_axis.z = 0.0;
        matrix3f.y_axis.x = 0.0;
        matrix3f.z_axis.x = 0.0;
        let f: f32 = self.cos();
        let g: f32 = self.sin();
        matrix3f.y_axis.y = f;
        matrix3f.z_axis.z = f;
        matrix3f.y_axis.z = g;
        matrix3f.z_axis.y = -g;
        matrix3f.x_axis.x = 1.0;
        return matrix3f.clone();
    }

    fn matrix_around_y(&self, matrix3f: &mut Mat3) -> Mat3 {
        matrix3f.x_axis.y = 0.0;
        matrix3f.y_axis.x = 0.0;
        matrix3f.y_axis.z = 0.0;
        matrix3f.z_axis.y = 0.0;
        let f: f32 = self.cos();
        let g: f32 = self.sin();
        matrix3f.x_axis.x = f;
        matrix3f.z_axis.z = f;
        matrix3f.x_axis.z = -g;
        matrix3f.z_axis.x = g;
        matrix3f.y_axis.y = 1.0;
        return matrix3f.clone();
    }

    fn matrix_around_z(&self, matrix3f: &mut Mat3) -> Mat3 {
        matrix3f.x_axis.z = 0.0;
        matrix3f.y_axis.z = 0.0;
        matrix3f.z_axis.x = 0.0;
        matrix3f.z_axis.y = 0.0;
        let f: f32 = self.cos();
        let g: f32 = self.sin();
        matrix3f.x_axis.x = f;
        matrix3f.y_axis.y = f;
        matrix3f.x_axis.y = g;
        matrix3f.y_axis.x = -g;
        matrix3f.z_axis.z = 1.0;
        return matrix3f.clone();
    }
}