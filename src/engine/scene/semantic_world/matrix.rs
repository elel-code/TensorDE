//! Matrix helpers for resolved scene semantic transforms.
//!
//! References:
//! - `docs/gilder-scene-engine-architecture.md`
//! - `reverse-engineered/docs/scene-format.md`
//! - `reverse-engineered/docs/exe/scene-and-object.md`

use super::components::TransformComponent;

pub fn transform_matrix(transform: &TransformComponent) -> [f32; 16] {
    let scale = scale_matrix(transform.scale.x, transform.scale.y, transform.scale.z);
    let rotation = euler_degrees_matrix(transform.angles.x, transform.angles.y, transform.angles.z);
    let translation =
        translation_matrix(transform.origin.x, transform.origin.y, transform.origin.z);
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

fn euler_degrees_matrix(pitch: f32, yaw: f32, roll: f32) -> [f32; 16] {
    let pitch = pitch.to_radians();
    let yaw = yaw.to_radians();
    let roll = roll.to_radians();
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
}
