//! Wallpaper Engine particle instance-color resolution.

use super::SceneVec3;

pub fn resolve_particle_color_range(
    min: SceneVec3,
    max: SceneVec3,
    reference: SceneVec3,
    instance: Option<SceneVec3>,
) -> (SceneVec3, SceneVec3) {
    let Some(instance) = instance else {
        return (min, max);
    };
    let reference_hsv = rgb_to_hsv(reference);
    let instance_hsv = rgb_to_hsv(instance);
    let offset = SceneVec3 {
        x: instance_hsv.x - reference_hsv.x,
        y: instance_hsv.y - reference_hsv.y,
        z: instance_hsv.z - reference_hsv.z,
    };
    (
        hsv_to_rgb(offset_hsv(rgb_to_hsv(min), offset)),
        hsv_to_rgb(offset_hsv(rgb_to_hsv(max), offset)),
    )
}

fn offset_hsv(endpoint: SceneVec3, offset: SceneVec3) -> SceneVec3 {
    SceneVec3 {
        x: (endpoint.x + offset.x).rem_euclid(1.0),
        y: (endpoint.y + offset.y).clamp(0.0, 1.0),
        z: (endpoint.z + offset.z).clamp(0.0, 1.0),
    }
}

fn rgb_to_hsv(rgb: SceneVec3) -> SceneVec3 {
    let max = rgb.x.max(rgb.y).max(rgb.z);
    let min = rgb.x.min(rgb.y).min(rgb.z);
    let chroma = max - min;
    let hue = if chroma == 0.0 {
        0.0
    } else if max == rgb.x {
        ((rgb.y - rgb.z) / chroma).rem_euclid(6.0) / 6.0
    } else if max == rgb.y {
        ((rgb.z - rgb.x) / chroma + 2.0) / 6.0
    } else {
        ((rgb.x - rgb.y) / chroma + 4.0) / 6.0
    };
    SceneVec3 {
        x: hue,
        y: if max == 0.0 { 0.0 } else { chroma / max },
        z: max,
    }
}

fn hsv_to_rgb(hsv: SceneVec3) -> SceneVec3 {
    let hue = hsv.x.rem_euclid(1.0) * 6.0;
    let chroma = hsv.z * hsv.y;
    let x = chroma * (1.0 - (hue.rem_euclid(2.0) - 1.0).abs());
    let (red, green, blue) = match hue.floor() as u32 {
        0 => (chroma, x, 0.0),
        1 => (x, chroma, 0.0),
        2 => (0.0, chroma, x),
        3 => (0.0, x, chroma),
        4 => (x, 0.0, chroma),
        _ => (chroma, 0.0, x),
    };
    let match_value = hsv.z - chroma;
    SceneVec3 {
        x: red + match_value,
        y: green + match_value,
        z: blue + match_value,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_vec3_close(actual: SceneVec3, expected: SceneVec3) {
        for (actual, expected) in [
            (actual.x, expected.x),
            (actual.y, expected.y),
            (actual.z, expected.z),
        ] {
            assert!((actual - expected).abs() < 1.0e-5, "{actual} != {expected}");
        }
    }

    #[test]
    fn absent_instance_color_preserves_authored_range() {
        let min = SceneVec3 {
            x: 110.0 / 255.0,
            y: 92.0 / 255.0,
            z: 20.0 / 255.0,
        };
        let max = SceneVec3 {
            x: 170.0 / 255.0,
            y: 110.0 / 255.0,
            z: 40.0 / 255.0,
        };
        assert_eq!(
            resolve_particle_color_range(min, max, min, None),
            (min, max)
        );
    }

    #[test]
    fn white_and_tinted_instance_colors_match_verified_we_endpoints() {
        let min = SceneVec3 {
            x: 110.0 / 255.0,
            y: 92.0 / 255.0,
            z: 20.0 / 255.0,
        };
        let max = SceneVec3 {
            x: 170.0 / 255.0,
            y: 110.0 / 255.0,
            z: 40.0 / 255.0,
        };
        let reference = SceneVec3 {
            x: (min.x + max.x) * 0.5,
            y: (min.y + max.y) * 0.5,
            z: (min.z + max.z) * 0.5,
        };
        let (white_min, white_max) =
            resolve_particle_color_range(min, max, reference, Some(SceneVec3::ONE));
        assert_vec3_close(
            white_min,
            SceneVec3 {
                x: 0.88235294,
                y: 0.85813251,
                z: 0.85370512,
            },
        );
        assert_vec3_close(white_max, SceneVec3::ONE);

        let (tinted_min, tinted_max) = resolve_particle_color_range(
            min,
            max,
            reference,
            Some(SceneVec3 {
                x: 0.5,
                y: 0.25,
                z: 0.75,
            }),
        );
        assert_vec3_close(
            tinted_min,
            SceneVec3 {
                x: 0.47962764,
                y: 0.19025337,
                z: 0.63235294,
            },
        );
        assert_vec3_close(
            tinted_max,
            SceneVec3 {
                x: 0.52760746,
                y: 0.30744357,
                z: 0.86764706,
            },
        );
    }
}
