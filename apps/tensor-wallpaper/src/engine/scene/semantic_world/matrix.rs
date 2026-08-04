//! Matrix helpers for resolved scene semantic transforms.
//!
//! References:
//! - `docs/tensor-wallpaper/tensor-wallpaper-scene-engine-architecture.md`
//! - `reverse-engineered/tensor-wallpaper/docs/scene-format.md`
//! - `reverse-engineered/tensor-wallpaper/docs/exe/scene-and-object.md`

use super::components::TransformComponent;
use crate::engine::scene::abi::SceneVec3;

pub fn transform_matrix(transform: &TransformComponent) -> [f32; 16] {
    compose_transform_matrix(transform.origin, transform.angles, transform.scale)
}

pub fn transform_matrix_radians(transform: &TransformComponent) -> [f32; 16] {
    transform_matrix(transform)
}

fn compose_transform_matrix(origin: SceneVec3, rotation: SceneVec3, scale: SceneVec3) -> [f32; 16] {
    let scale = scale_matrix(scale.x, scale.y, scale.z);
    let rotation = euler_radians_matrix(rotation.x, rotation.y, rotation.z);
    let translation = translation_matrix(origin.x, origin.y, origin.z);
    multiply_matrix(&translation, &multiply_matrix(&rotation, &scale))
}

pub fn multiply_matrix(lhs: &[f32; 16], rhs: &[f32; 16]) -> [f32; 16] {
    let mut out = [0.0; 16];
    for col in 0..4 {
        for row in 0..4 {
            out[col * 4 + row] = lhs[row] * rhs[col * 4]
                + lhs[4 + row] * rhs[col * 4 + 1]
                + lhs[8 + row] * rhs[col * 4 + 2]
                + lhs[12 + row] * rhs[col * 4 + 3];
        }
    }
    out
}

pub fn identity_matrix() -> [f32; 16] {
    [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}

pub fn inverse_affine_matrix(matrix: &[f32; 16]) -> Option<[f32; 16]> {
    let (a00, a01, a02) = (matrix[0], matrix[4], matrix[8]);
    let (a10, a11, a12) = (matrix[1], matrix[5], matrix[9]);
    let (a20, a21, a22) = (matrix[2], matrix[6], matrix[10]);
    let determinant = a00 * (a11 * a22 - a12 * a21) - a01 * (a10 * a22 - a12 * a20)
        + a02 * (a10 * a21 - a11 * a20);
    if !determinant.is_finite() || determinant.abs() <= 1.0e-8 {
        return None;
    }
    let inverse_determinant = determinant.recip();
    let i00 = (a11 * a22 - a12 * a21) * inverse_determinant;
    let i01 = (a02 * a21 - a01 * a22) * inverse_determinant;
    let i02 = (a01 * a12 - a02 * a11) * inverse_determinant;
    let i10 = (a12 * a20 - a10 * a22) * inverse_determinant;
    let i11 = (a00 * a22 - a02 * a20) * inverse_determinant;
    let i12 = (a02 * a10 - a00 * a12) * inverse_determinant;
    let i20 = (a10 * a21 - a11 * a20) * inverse_determinant;
    let i21 = (a01 * a20 - a00 * a21) * inverse_determinant;
    let i22 = (a00 * a11 - a01 * a10) * inverse_determinant;
    let (tx, ty, tz) = (matrix[12], matrix[13], matrix[14]);
    let inverse = [
        i00,
        i10,
        i20,
        0.0,
        i01,
        i11,
        i21,
        0.0,
        i02,
        i12,
        i22,
        0.0,
        -(i00 * tx + i01 * ty + i02 * tz),
        -(i10 * tx + i11 * ty + i12 * tz),
        -(i20 * tx + i21 * ty + i22 * tz),
        1.0,
    ];
    inverse
        .iter()
        .all(|value| value.is_finite())
        .then_some(inverse)
}

pub fn interpolate_affine_matrix(from: &[f32; 16], to: &[f32; 16], weight: f32) -> [f32; 16] {
    let weight = if weight.is_finite() {
        weight.clamp(0.0, 1.0)
    } else {
        0.0
    };
    if weight == 0.0 {
        return *from;
    }
    if weight == 1.0 {
        return *to;
    }
    let (Some(from), Some(to)) = (decompose_affine_matrix(from), decompose_affine_matrix(to))
    else {
        return std::array::from_fn(|index| from[index] * (1.0 - weight) + to[index] * weight);
    };
    let rotation = normalized_quaternion_lerp(from.rotation, to.rotation, weight);
    compose_affine_components(AffineComponents {
        translation: lerp_vec3(from.translation, to.translation, weight),
        rotation,
        scale: lerp_vec3(from.scale, to.scale, weight),
    })
}

#[derive(Clone, Copy)]
struct AffineComponents {
    translation: [f32; 3],
    rotation: [f32; 4],
    scale: [f32; 3],
}

fn decompose_affine_matrix(matrix: &[f32; 16]) -> Option<AffineComponents> {
    if !matrix.iter().all(|value| value.is_finite()) {
        return None;
    }
    let mut column_x = [matrix[0], matrix[1], matrix[2]];
    let column_y = [matrix[4], matrix[5], matrix[6]];
    let column_z = [matrix[8], matrix[9], matrix[10]];
    let mut scale = [length3(column_x), length3(column_y), length3(column_z)];
    if scale.iter().any(|value| *value <= 1.0e-8) {
        return None;
    }
    if dot3(cross3(column_x, column_y), column_z) < 0.0 {
        scale[0] = -scale[0];
        column_x = [-column_x[0], -column_x[1], -column_x[2]];
    }
    let rotation_columns = [
        scale3(column_x, scale[0].abs().recip()),
        scale3(column_y, scale[1].recip()),
        scale3(column_z, scale[2].recip()),
    ];
    if dot3(rotation_columns[0], rotation_columns[1]).abs() > 1.0e-3
        || dot3(rotation_columns[0], rotation_columns[2]).abs() > 1.0e-3
        || dot3(rotation_columns[1], rotation_columns[2]).abs() > 1.0e-3
    {
        return None;
    }
    Some(AffineComponents {
        translation: [matrix[12], matrix[13], matrix[14]],
        rotation: quaternion_from_columns(rotation_columns)?,
        scale,
    })
}

fn compose_affine_components(components: AffineComponents) -> [f32; 16] {
    let [x, y, z, w] = components.rotation;
    let [sx, sy, sz] = components.scale;
    let r00 = 1.0 - 2.0 * (y * y + z * z);
    let r01 = 2.0 * (x * y - z * w);
    let r02 = 2.0 * (x * z + y * w);
    let r10 = 2.0 * (x * y + z * w);
    let r11 = 1.0 - 2.0 * (x * x + z * z);
    let r12 = 2.0 * (y * z - x * w);
    let r20 = 2.0 * (x * z - y * w);
    let r21 = 2.0 * (y * z + x * w);
    let r22 = 1.0 - 2.0 * (x * x + y * y);
    [
        r00 * sx,
        r10 * sx,
        r20 * sx,
        0.0,
        r01 * sy,
        r11 * sy,
        r21 * sy,
        0.0,
        r02 * sz,
        r12 * sz,
        r22 * sz,
        0.0,
        components.translation[0],
        components.translation[1],
        components.translation[2],
        1.0,
    ]
}

fn quaternion_from_columns(columns: [[f32; 3]; 3]) -> Option<[f32; 4]> {
    let (r00, r01, r02) = (columns[0][0], columns[1][0], columns[2][0]);
    let (r10, r11, r12) = (columns[0][1], columns[1][1], columns[2][1]);
    let (r20, r21, r22) = (columns[0][2], columns[1][2], columns[2][2]);
    let trace = r00 + r11 + r22;
    let quaternion = if trace > 0.0 {
        let s = (trace + 1.0).sqrt() * 2.0;
        [(r21 - r12) / s, (r02 - r20) / s, (r10 - r01) / s, 0.25 * s]
    } else if r00 > r11 && r00 > r22 {
        let s = (1.0 + r00 - r11 - r22).sqrt() * 2.0;
        [0.25 * s, (r01 + r10) / s, (r02 + r20) / s, (r21 - r12) / s]
    } else if r11 > r22 {
        let s = (1.0 + r11 - r00 - r22).sqrt() * 2.0;
        [(r01 + r10) / s, 0.25 * s, (r12 + r21) / s, (r02 - r20) / s]
    } else {
        let s = (1.0 + r22 - r00 - r11).sqrt() * 2.0;
        [(r02 + r20) / s, (r12 + r21) / s, 0.25 * s, (r10 - r01) / s]
    };
    normalize_quaternion(quaternion)
}

fn normalized_quaternion_lerp(mut from: [f32; 4], mut to: [f32; 4], weight: f32) -> [f32; 4] {
    if dot4(from, to) < 0.0 {
        to = [-to[0], -to[1], -to[2], -to[3]];
    }
    for index in 0..4 {
        from[index] = from[index] * (1.0 - weight) + to[index] * weight;
    }
    normalize_quaternion(from).unwrap_or([0.0, 0.0, 0.0, 1.0])
}

fn normalize_quaternion(value: [f32; 4]) -> Option<[f32; 4]> {
    let length = dot4(value, value).sqrt();
    (length > 1.0e-8).then(|| {
        [
            value[0] / length,
            value[1] / length,
            value[2] / length,
            value[3] / length,
        ]
    })
}

fn lerp_vec3(from: [f32; 3], to: [f32; 3], weight: f32) -> [f32; 3] {
    std::array::from_fn(|index| from[index] * (1.0 - weight) + to[index] * weight)
}

fn scale3(value: [f32; 3], scale: f32) -> [f32; 3] {
    [value[0] * scale, value[1] * scale, value[2] * scale]
}

fn length3(value: [f32; 3]) -> f32 {
    dot3(value, value).sqrt()
}

fn dot3(lhs: [f32; 3], rhs: [f32; 3]) -> f32 {
    lhs[0] * rhs[0] + lhs[1] * rhs[1] + lhs[2] * rhs[2]
}

fn dot4(lhs: [f32; 4], rhs: [f32; 4]) -> f32 {
    lhs[0] * rhs[0] + lhs[1] * rhs[1] + lhs[2] * rhs[2] + lhs[3] * rhs[3]
}

fn cross3(lhs: [f32; 3], rhs: [f32; 3]) -> [f32; 3] {
    [
        lhs[1] * rhs[2] - lhs[2] * rhs[1],
        lhs[2] * rhs[0] - lhs[0] * rhs[2],
        lhs[0] * rhs[1] - lhs[1] * rhs[0],
    ]
}

fn translation_matrix(x: f32, y: f32, z: f32) -> [f32; 16] {
    [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, x, y, z, 1.0,
    ]
}

fn scale_matrix(x: f32, y: f32, z: f32) -> [f32; 16] {
    [
        x, 0.0, 0.0, 0.0, 0.0, y, 0.0, 0.0, 0.0, 0.0, z, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}

fn euler_radians_matrix(pitch: f32, yaw: f32, roll: f32) -> [f32; 16] {
    let (sin_pitch, cos_pitch) = pitch.sin_cos();
    let (sin_yaw, cos_yaw) = yaw.sin_cos();
    let (sin_roll, cos_roll) = roll.sin_cos();

    let pitch_matrix = [
        1.0, 0.0, 0.0, 0.0, 0.0, cos_pitch, sin_pitch, 0.0, 0.0, -sin_pitch, cos_pitch, 0.0, 0.0,
        0.0, 0.0, 1.0,
    ];
    let yaw_matrix = [
        cos_yaw, 0.0, -sin_yaw, 0.0, 0.0, 1.0, 0.0, 0.0, sin_yaw, 0.0, cos_yaw, 0.0, 0.0, 0.0, 0.0,
        1.0,
    ];
    let roll_matrix = [
        cos_roll, sin_roll, 0.0, 0.0, -sin_roll, cos_roll, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0,
        0.0, 1.0,
    ];

    multiply_matrix(&roll_matrix, &multiply_matrix(&yaw_matrix, &pitch_matrix))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene::SceneVec3;

    #[test]
    fn transform_matrix_places_translation_in_column_major_slot() {
        let matrix = transform_matrix(&TransformComponent {
            origin: SceneVec3 {
                x: 3.0,
                y: 4.0,
                z: 5.0,
            },
            angles: SceneVec3::default(),
            scale: SceneVec3 {
                x: 2.0,
                y: 3.0,
                z: 4.0,
            },
            camera_zoom: 1.0,
        });

        assert_eq!(matrix[0], 2.0);
        assert_eq!(matrix[5], 3.0);
        assert_eq!(matrix[10], 4.0);
        assert_eq!(matrix[12], 3.0);
        assert_eq!(matrix[13], 4.0);
        assert_eq!(matrix[14], 5.0);
    }

    #[test]
    fn matrix_multiply_composes_parent_and_child_translation() {
        let parent = translation_matrix(10.0, 20.0, 0.0);
        let child = translation_matrix(5.0, 7.0, 0.0);
        let world = multiply_matrix(&parent, &child);

        assert_eq!(world[12], 15.0);
        assert_eq!(world[13], 27.0);
    }

    #[test]
    fn scene_object_angles_are_radians() {
        let matrix = transform_matrix(&TransformComponent {
            origin: SceneVec3::default(),
            angles: SceneVec3 {
                x: 0.0,
                y: 0.0,
                z: -std::f32::consts::FRAC_PI_2,
            },
            scale: SceneVec3::ONE,
            camera_zoom: 1.0,
        });

        assert!(matrix[0].abs() <= 1.0e-5);
        assert!((matrix[1] + 1.0).abs() <= 1.0e-5);
        assert!((matrix[4] - 1.0).abs() <= 1.0e-5);
        assert!(matrix[5].abs() <= 1.0e-5);
    }

    #[test]
    fn affine_inverse_cancels_translation_rotation_and_scale() {
        let transform = transform_matrix(&TransformComponent {
            origin: SceneVec3 {
                x: 10.0,
                y: -4.0,
                z: 2.0,
            },
            angles: SceneVec3 {
                x: 0.25,
                y: -0.35,
                z: 0.6,
            },
            scale: SceneVec3 {
                x: 2.0,
                y: 0.5,
                z: 1.5,
            },
            camera_zoom: 1.0,
        });
        let inverse = inverse_affine_matrix(&transform).expect("invertible");
        let product = multiply_matrix(&transform, &inverse);

        for (index, value) in product.into_iter().enumerate() {
            let expected = f32::from(matches!(index, 0 | 5 | 10 | 15));
            assert!((value - expected).abs() < 1.0e-4, "index {index}: {value}");
        }
    }

    #[test]
    fn animation_transform_keeps_mdla_rotation_in_radians() {
        let matrix = transform_matrix_radians(&TransformComponent {
            origin: SceneVec3::default(),
            angles: SceneVec3 {
                x: 0.0,
                y: 0.0,
                z: std::f32::consts::FRAC_PI_2,
            },
            scale: SceneVec3 {
                x: 1.0,
                y: 1.0,
                z: 1.0,
            },
            camera_zoom: 1.0,
        });

        assert!(matrix[0].abs() < 1.0e-6);
        assert!((matrix[1] - 1.0).abs() < 1.0e-6);
        assert!((matrix[4] + 1.0).abs() < 1.0e-6);
        assert!(matrix[5].abs() < 1.0e-6);
    }

    #[test]
    fn affine_interpolation_blends_translation_and_rotation() {
        let target = transform_matrix_radians(&TransformComponent {
            origin: SceneVec3 {
                x: 10.0,
                y: 4.0,
                z: 0.0,
            },
            angles: SceneVec3 {
                x: 0.0,
                y: 0.0,
                z: std::f32::consts::FRAC_PI_2,
            },
            scale: SceneVec3 {
                x: 1.0,
                y: 1.0,
                z: 1.0,
            },
            camera_zoom: 1.0,
        });
        let blended = interpolate_affine_matrix(&identity_matrix(), &target, 0.5);
        let diagonal = std::f32::consts::FRAC_1_SQRT_2;

        assert!((blended[0] - diagonal).abs() < 1.0e-5);
        assert!((blended[1] - diagonal).abs() < 1.0e-5);
        assert!((blended[12] - 5.0).abs() < 1.0e-5);
        assert!((blended[13] - 2.0).abs() < 1.0e-5);
    }
}
