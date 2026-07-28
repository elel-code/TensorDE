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
