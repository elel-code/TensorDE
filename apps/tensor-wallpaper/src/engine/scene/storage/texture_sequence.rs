//! Compact GPU layout derived from exact authored TEXS frame affines.

use super::SceneTextureSequenceFrameRecord;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SceneTextureSequenceLayout {
    pub origin: [f32; 2],
    pub frame_size: [f32; 2],
    pub row_stride: u32,
}

pub(crate) fn texture_sequence_layout(
    frames: &[SceneTextureSequenceFrameRecord],
) -> Option<SceneTextureSequenceLayout> {
    let first = *frames.first()?;
    let close = |left: f32, right: f32| (left - right).abs() <= 1.0e-4;
    let row_stride = frames
        .iter()
        .enumerate()
        .skip(1)
        .find(|(_, frame)| !close(frame.origin[1], first.origin[1]))
        .map_or(frames.len(), |(index, _)| index);
    let row_stride = u32::try_from(row_stride).ok()?;
    if row_stride == 0 || first.axis_x[0] <= 0.0 || first.axis_y[1] <= 0.0 {
        return None;
    }
    for (index, frame) in frames.iter().enumerate() {
        let index = u32::try_from(index).ok()?;
        let expected_origin = [
            first.origin[0] + (index % row_stride) as f32 * first.axis_x[0],
            first.origin[1] + (index / row_stride) as f32 * first.axis_y[1],
        ];
        if !close(frame.origin[0], expected_origin[0])
            || !close(frame.origin[1], expected_origin[1])
            || !close(frame.axis_x[0], first.axis_x[0])
            || !close(frame.axis_x[1], 0.0)
            || !close(frame.axis_y[0], 0.0)
            || !close(frame.axis_y[1], first.axis_y[1])
        {
            return None;
        }
    }
    Some(SceneTextureSequenceLayout {
        origin: first.origin,
        frame_size: [first.axis_x[0], first.axis_y[1]],
        row_stride,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_preserves_an_authored_row_stride_independent_of_frame_width() {
        let frames = (0..30)
            .map(|index| SceneTextureSequenceFrameRecord {
                resource_index: 0,
                duration: 1.0 / 30.0,
                origin: [(index % 5) as f32 / 6.0, (index / 5) as f32 / 5.0],
                axis_x: [1.0 / 6.0, 0.0],
                axis_y: [0.0, 1.0 / 5.0],
            })
            .collect::<Vec<_>>();

        assert_eq!(
            texture_sequence_layout(&frames),
            Some(SceneTextureSequenceLayout {
                origin: [0.0, 0.0],
                frame_size: [1.0 / 6.0, 1.0 / 5.0],
                row_stride: 5,
            })
        );
    }

    #[test]
    fn layout_rejects_a_frame_outside_the_authored_row_major_affine() {
        let mut frames = vec![
            SceneTextureSequenceFrameRecord {
                resource_index: 0,
                duration: 1.0,
                origin: [0.0, 0.0],
                axis_x: [0.5, 0.0],
                axis_y: [0.0, 0.5],
            };
            4
        ];
        frames[1].origin = [0.5, 0.0];
        frames[2].origin = [0.0, 0.5];
        frames[3].origin = [0.75, 0.5];

        assert_eq!(texture_sequence_layout(&frames), None);
    }
}
