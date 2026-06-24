use std::path::PathBuf;

use crate::core::FitMode;
use crate::renderer::SceneLiteDisplayPlan;

use super::{NativeVulkanClearColor, NativeVulkanRenderItem};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct NativeVulkanStaticUploadPlan {
    pub(super) source: PathBuf,
    pub(super) fit: FitMode,
    pub(super) background: Option<String>,
}

pub(super) fn native_vulkan_static_upload_plan(
    render_item: &NativeVulkanRenderItem,
) -> Option<NativeVulkanStaticUploadPlan> {
    match render_item {
        NativeVulkanRenderItem::StaticImage {
            source,
            fit,
            background,
            ..
        } => Some(NativeVulkanStaticUploadPlan {
            source: source.clone(),
            fit: *fit,
            background: background.clone(),
        }),
        NativeVulkanRenderItem::Video {
            poster: Some(poster),
            fit,
            ..
        } => Some(NativeVulkanStaticUploadPlan {
            source: poster.clone(),
            fit: *fit,
            background: None,
        }),
        NativeVulkanRenderItem::SceneLite {
            display:
                Some(SceneLiteDisplayPlan::Image {
                    source,
                    fit,
                    background,
                }),
            ..
        } => Some(NativeVulkanStaticUploadPlan {
            source: source.clone(),
            fit: *fit,
            background: background.clone(),
        }),
        _ => None,
    }
}

pub(super) fn native_vulkan_render_item_clear_color(
    render_item: &NativeVulkanRenderItem,
    fallback: NativeVulkanClearColor,
) -> NativeVulkanClearColor {
    match render_item {
        NativeVulkanRenderItem::SceneLite {
            display: Some(SceneLiteDisplayPlan::Color { color }),
            ..
        } => native_vulkan_clear_color_from_hex(color).unwrap_or(fallback),
        _ => fallback,
    }
}

fn native_vulkan_clear_color_from_hex(value: &str) -> Option<NativeVulkanClearColor> {
    let hex = value.trim().strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()? as f32 / 255.0;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()? as f32 / 255.0;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()? as f32 / 255.0;
    Some(NativeVulkanClearColor { r, g, b, a: 1.0 })
}
