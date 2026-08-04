use super::*;

#[test]
fn tessellation_is_independent_of_a_renderer_backend() {
    let geometry = tessellate_svg(
        br##"<svg viewBox="0 0 32 24"><rect width="32" height="24" fill="#47a3ff"/></svg>"##,
        64,
        64,
    )
    .expect("valid SVG geometry");

    assert!(!geometry.vertices.is_empty());
    assert!(!geometry.indices.is_empty());
    assert!(geometry.vertices.iter().all(|vertex| {
        vertex.position.into_iter().all(f32::is_finite)
            && vertex.color.into_iter().all(f32::is_finite)
    }));
}

#[test]
fn zero_sized_target_never_builds_gpu_geometry() {
    let svg = br#"<svg width="16" height="16"><circle cx="8" cy="8" r="8"/></svg>"#;
    assert!(tessellate_svg(svg, 0, 16).is_none());
    assert!(tessellate_svg(svg, 16, 0).is_none());
}

#[test]
fn svg_top_edge_maps_to_vulkan_top_edge() {
    let geometry = tessellate_svg(
        br##"<svg viewBox="0 0 10 10"><rect y="0" width="10" height="2" fill="#fff"/></svg>"##,
        100,
        100,
    )
    .expect("valid top-edge SVG geometry");
    let min_y = geometry
        .vertices
        .iter()
        .map(|vertex| vertex.position[1])
        .fold(f32::INFINITY, f32::min);
    let max_y = geometry
        .vertices
        .iter()
        .map(|vertex| vertex.position[1])
        .fold(f32::NEG_INFINITY, f32::max);

    assert!((min_y + 1.0).abs() < 1.0e-5);
    assert!(max_y < 0.0, "top SVG content must stay in the top half");
}
