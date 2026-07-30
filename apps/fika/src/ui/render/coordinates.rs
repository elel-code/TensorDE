use crate::windowing::PhysicalSize;
use fika_core::ViewRect;

/// Converts Fika's top-left screen coordinates directly to Vulkan NDC.
///
/// With a positive-height Vulkan viewport, the framebuffer's top-left and
/// bottom-right corners are `(-1, -1)` and `(1, 1)` respectively.
pub(crate) fn rect_to_vulkan_ndc(rect: ViewRect, size: PhysicalSize<u32>) -> [f32; 4] {
    let width = size.width.max(1) as f32;
    let height = size.height.max(1) as f32;
    [
        rect.x / width * 2.0 - 1.0,
        rect.y / height * 2.0 - 1.0,
        rect.right() / width * 2.0 - 1.0,
        rect.bottom() / height * 2.0 - 1.0,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screen_rects_map_to_vulkan_native_y_down_ndc() {
        let size = PhysicalSize::new(800, 400);
        assert_eq!(
            rect_to_vulkan_ndc(
                ViewRect {
                    x: 0.0,
                    y: 0.0,
                    width: 800.0,
                    height: 400.0,
                },
                size,
            ),
            [-1.0, -1.0, 1.0, 1.0]
        );
        let rect = rect_to_vulkan_ndc(
            ViewRect {
                x: 100.0,
                y: 50.0,
                width: 200.0,
                height: 100.0,
            },
            size,
        );
        assert_eq!(rect, [-0.75, -0.75, -0.25, -0.25]);
        assert!(rect[1] < rect[3], "top must precede bottom in Vulkan NDC");
    }
}
