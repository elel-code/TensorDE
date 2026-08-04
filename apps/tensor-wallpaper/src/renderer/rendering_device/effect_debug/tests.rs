use super::*;

#[test]
fn r8_group_report_includes_sample_and_coverage_labels() {
    let payload = [255, 0, 64, 128, 192, 255];
    let samples = [[0.0, 0.0], [1.0, 1.0]];
    let coverage = [[0.0, 0.0], [0.5, 0.5], [1.0, 1.0]];
    let group = RenderingDeviceEffectDebugR8UvGroup {
        label: "current",
        sample_uvs: &samples,
        coverage_uvs: &coverage,
    };

    let report = rendering_device_effect_debug_r8_payload_group_report(3, 2, &payload, &[group]);

    assert!(report.contains("current_samples=["));
    assert!(report.contains("current_coverage=n=3"));
    assert!(report.contains("gt127="));
}
