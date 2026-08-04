use super::*;

#[test]
fn fourcc_maps_native_vulkan_formats_and_xrgb_alpha() {
    let (argb, argb_components) =
        vulkan_format_for_fourcc(fourcc::ARGB8888).expect("ARGB8888 Vulkan format");
    assert_eq!(argb, vulkan_renderer::TextureFormat::Bgra8Unorm);
    assert_eq!(
        argb_components.alpha,
        vulkan_renderer::ComponentSwizzle::Alpha
    );

    let (xrgb, xrgb_components) =
        vulkan_format_for_fourcc(fourcc::XRGB8888).expect("XRGB8888 Vulkan format");
    assert_eq!(xrgb, vulkan_renderer::TextureFormat::Bgra8Unorm);
    assert_eq!(
        xrgb_components.alpha,
        vulkan_renderer::ComponentSwizzle::One
    );

    let (rgba, _) = vulkan_format_for_fourcc(fourcc::RGBA8888).expect("RGBA8888 Vulkan format");
    assert_eq!(rgba, vulkan_renderer::TextureFormat::Rgba8Unorm);
    assert!(vulkan_format_for_fourcc(0xdead_beef).is_none());
}

#[test]
fn pick_import_format_prefers_argb_from_feedback() {
    use wayland_client_runtime::{DmabufFeedback, DmabufFeedbackTranche, DmabufFormat};

    let feedback = DmabufFeedback {
        main_device: 0,
        formats: vec![
            DmabufFormat::new(fourcc::RGBA8888, fourcc::MOD_LINEAR),
            DmabufFormat::new(fourcc::ARGB8888, fourcc::MOD_LINEAR),
        ],
        tranches: vec![DmabufFeedbackTranche {
            device: 0,
            flags: wayland_client_runtime::DmabufTrancheFlags::empty(),
            formats: vec![0, 1],
        }],
    };
    let picked = pick_import_format(&feedback).expect("pick");
    assert_eq!(picked.format, fourcc::ARGB8888);
}

#[test]
fn pick_export_format_skips_implicit_and_non_exportable_modifiers() {
    use wayland_client_runtime::{DmabufFeedback, DmabufFeedbackTranche, DmabufFormat};

    let explicit = 0x0300_0000_0060_6010;
    let unsupported = 0x0300_0000_0060_6020;
    let feedback = DmabufFeedback {
        main_device: 0,
        formats: vec![
            DmabufFormat::new(fourcc::ARGB8888, fourcc::MOD_INVALID),
            DmabufFormat::new(fourcc::ARGB8888, unsupported),
            DmabufFormat::new(fourcc::ARGB8888, explicit),
            DmabufFormat::new(fourcc::RGBA8888, fourcc::MOD_LINEAR),
        ],
        tranches: vec![DmabufFeedbackTranche {
            device: 0,
            flags: wayland_client_runtime::DmabufTrancheFlags::empty(),
            formats: vec![0, 1, 2, 3],
        }],
    };
    let exportable = [
        DmabufFormat::new(fourcc::ARGB8888, explicit),
        DmabufFormat::new(fourcc::RGBA8888, fourcc::MOD_LINEAR),
    ];

    let picked = pick_export_format(&feedback, &exportable).expect("explicit export format");
    assert_eq!(picked.format, fourcc::ARGB8888);
    assert_eq!(picked.modifier, explicit);
}

#[test]
fn pick_export_format_requires_an_exact_compositor_vulkan_intersection() {
    use wayland_client_runtime::{DmabufFeedback, DmabufFormat};

    let feedback = DmabufFeedback {
        main_device: 0,
        formats: vec![DmabufFormat::new(fourcc::ARGB8888, fourcc::MOD_INVALID)],
        tranches: Vec::new(),
    };
    let exportable = [DmabufFormat::new(fourcc::ARGB8888, fourcc::MOD_LINEAR)];

    assert!(pick_export_format(&feedback, &exportable).is_none());
}
