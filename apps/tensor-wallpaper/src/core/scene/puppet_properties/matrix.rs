fn scene_puppet_world_matrices<P, M>(parents: P, local_matrices: M) -> Option<Vec<[f64; 16]>>
where
    P: IntoIterator<Item = Option<usize>>,
    M: IntoIterator<Item = [f64; 16]>,
{
    let parents = parents.into_iter().collect::<Vec<_>>();
    let locals = local_matrices.into_iter().collect::<Vec<_>>();
    if parents.len() != locals.len() {
        return None;
    }
    let mut worlds = vec![scene_puppet_identity_matrix(); locals.len()];
    for index in 0..locals.len() {
        worlds[index] = if let Some(parent) = parents[index] {
            if parent >= index {
                return None;
            }
            scene_puppet_matrix_mul(worlds[parent], locals[index])
        } else {
            locals[index]
        };
    }
    Some(worlds)
}

fn scene_puppet_identity_matrix() -> [f64; 16] {
    [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}

fn scene_puppet_translation_matrix(translation: [f64; 3]) -> [f64; 16] {
    let mut matrix = scene_puppet_identity_matrix();
    matrix[12] = translation[0];
    matrix[13] = translation[1];
    matrix[14] = translation[2];
    matrix
}

fn scene_puppet_scale_matrix(scale: [f64; 3]) -> [f64; 16] {
    [
        scale[0], 0.0, 0.0, 0.0, 0.0, scale[1], 0.0, 0.0, 0.0, 0.0, scale[2], 0.0, 0.0, 0.0, 0.0,
        1.0,
    ]
}

fn scene_puppet_rotation_x_matrix(angle: f64) -> [f64; 16] {
    let (sin, cos) = angle.sin_cos();
    [
        1.0, 0.0, 0.0, 0.0, 0.0, cos, sin, 0.0, 0.0, -sin, cos, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}

fn scene_puppet_rotation_y_matrix(angle: f64) -> [f64; 16] {
    let (sin, cos) = angle.sin_cos();
    [
        cos, 0.0, -sin, 0.0, 0.0, 1.0, 0.0, 0.0, sin, 0.0, cos, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}

fn scene_puppet_rotation_z_matrix(angle: f64) -> [f64; 16] {
    let (sin, cos) = angle.sin_cos();
    [
        cos, sin, 0.0, 0.0, -sin, cos, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}

fn scene_puppet_matrix_mul(a: [f64; 16], b: [f64; 16]) -> [f64; 16] {
    let mut output = [0.0; 16];
    for column in 0..4 {
        for row in 0..4 {
            output[column * 4 + row] = (0..4)
                .map(|index| a[index * 4 + row] * b[column * 4 + index])
                .sum();
        }
    }
    output
}

fn scene_puppet_inverse_affine_matrix(matrix: [f64; 16]) -> Option<[f64; 16]> {
    let a00 = matrix[0];
    let a01 = matrix[4];
    let a02 = matrix[8];
    let a10 = matrix[1];
    let a11 = matrix[5];
    let a12 = matrix[9];
    let a20 = matrix[2];
    let a21 = matrix[6];
    let a22 = matrix[10];
    let det = a00 * (a11 * a22 - a12 * a21) - a01 * (a10 * a22 - a12 * a20)
        + a02 * (a10 * a21 - a11 * a20);
    if !det.is_finite() || det.abs() <= f64::EPSILON {
        return None;
    }
    let inv_det = 1.0 / det;
    let b00 = (a11 * a22 - a12 * a21) * inv_det;
    let b01 = (a02 * a21 - a01 * a22) * inv_det;
    let b02 = (a01 * a12 - a02 * a11) * inv_det;
    let b10 = (a12 * a20 - a10 * a22) * inv_det;
    let b11 = (a00 * a22 - a02 * a20) * inv_det;
    let b12 = (a02 * a10 - a00 * a12) * inv_det;
    let b20 = (a10 * a21 - a11 * a20) * inv_det;
    let b21 = (a01 * a20 - a00 * a21) * inv_det;
    let b22 = (a00 * a11 - a01 * a10) * inv_det;
    let tx = matrix[12];
    let ty = matrix[13];
    let tz = matrix[14];
    Some([
        b00,
        b10,
        b20,
        0.0,
        b01,
        b11,
        b21,
        0.0,
        b02,
        b12,
        b22,
        0.0,
        -(b00 * tx + b01 * ty + b02 * tz),
        -(b10 * tx + b11 * ty + b12 * tz),
        -(b20 * tx + b21 * ty + b22 * tz),
        1.0,
    ])
}

fn scene_puppet_transform_point_3d(matrix: [f64; 16], x: f64, y: f64, z: f64) -> [f64; 3] {
    [
        matrix[0] * x + matrix[4] * y + matrix[8] * z + matrix[12],
        matrix[1] * x + matrix[5] * y + matrix[9] * z + matrix[13],
        matrix[2] * x + matrix[6] * y + matrix[10] * z + matrix[14],
    ]
}

fn scene_puppet_matrix_rotation_z(matrix: [f64; 16]) -> Option<f64> {
    let scale_x = (matrix[0] * matrix[0] + matrix[1] * matrix[1])
        .sqrt()
        .max(f64::EPSILON);
    let angle = (matrix[1] / scale_x).atan2(matrix[0] / scale_x);
    angle.is_finite().then_some(angle)
}
