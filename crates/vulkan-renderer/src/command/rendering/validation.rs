use vulkanalia::vk;

use super::{AttachmentView, RenderingDescriptor, ResolveMode};
use crate::command::CommandEncoder;
use crate::pipeline::{format_has_depth, format_has_stencil};
use crate::{Error, Features, Result, SampleCount, TextureFormat};

pub(super) struct RenderingMetadata {
    pub(super) color_formats: Vec<Option<TextureFormat>>,
    pub(super) depth_format: vk::Format,
    pub(super) stencil_format: vk::Format,
    pub(super) sample_count: vk::SampleCountFlags,
}

pub(super) fn validate_descriptor(
    encoder: &CommandEncoder,
    descriptor: &RenderingDescriptor<'_>,
) -> Result<RenderingMetadata> {
    if descriptor.render_area.extent.is_empty() {
        return Err(Error::Validation(
            "dynamic rendering area must be non-empty".into(),
        ));
    }
    if descriptor.layer_count == 0 {
        return Err(Error::Validation(
            "dynamic rendering layer_count must be non-zero".into(),
        ));
    }
    if descriptor.color_attachments.is_empty()
        && descriptor.depth_attachment.is_none()
        && descriptor.stencil_attachment.is_none()
    {
        return Err(Error::Validation(
            "dynamic rendering requires at least one attachment slot".into(),
        ));
    }

    let mut sample_count = None;
    let mut color_formats = Vec::with_capacity(descriptor.color_attachments.len());
    for attachment in descriptor.color_attachments {
        match attachment {
            Some(attachment) => {
                if format_has_depth(attachment.view.format())
                    || format_has_stencil(attachment.view.format())
                {
                    return Err(Error::Validation(
                        "color attachment format must not contain depth or stencil aspects".into(),
                    ));
                }
                validate_attachment(
                    encoder,
                    attachment.view,
                    attachment.layout,
                    attachment.resolve_target,
                    attachment.resolve_layout,
                    attachment.resolve_mode,
                    &mut sample_count,
                )?;
                let format = TextureFormat::from_vk(attachment.view.format()).ok_or_else(|| {
                    Error::Validation(
                        "color attachment format is not represented by TextureFormat".into(),
                    )
                })?;
                color_formats.push(Some(format));
            }
            None => color_formats.push(None),
        }
    }
    if let Some(attachment) = descriptor.depth_attachment {
        if !format_has_depth(attachment.view.format()) {
            return Err(Error::Validation(
                "depth attachment format must contain a depth aspect".into(),
            ));
        }
        validate_attachment(
            encoder,
            attachment.view,
            attachment.layout,
            attachment.resolve_target,
            attachment.resolve_layout,
            attachment.resolve_mode,
            &mut sample_count,
        )?;
    }
    if let Some(attachment) = descriptor.stencil_attachment {
        if !format_has_stencil(attachment.view.format()) {
            return Err(Error::Validation(
                "stencil attachment format must contain a stencil aspect".into(),
            ));
        }
        validate_attachment(
            encoder,
            attachment.view,
            attachment.layout,
            attachment.resolve_target,
            attachment.resolve_layout,
            attachment.resolve_mode,
            &mut sample_count,
        )?;
    }
    let sample_count = sample_count.unwrap_or(vk::SampleCountFlags::_1);
    let pipeline_sample_count = match descriptor.multisampled_render_to_single_sampled {
        Some(count) => {
            validate_render_to_single_sampled(encoder, descriptor, count, sample_count)?;
            count.to_vk()
        }
        None => sample_count,
    };
    Ok(RenderingMetadata {
        color_formats,
        depth_format: descriptor
            .depth_attachment
            .map_or(vk::Format::UNDEFINED, |attachment| attachment.view.format()),
        stencil_format: descriptor
            .stencil_attachment
            .map_or(vk::Format::UNDEFINED, |attachment| attachment.view.format()),
        sample_count: pipeline_sample_count,
    })
}

fn validate_render_to_single_sampled(
    encoder: &CommandEncoder,
    descriptor: &RenderingDescriptor<'_>,
    count: SampleCount,
    attachment_sample_count: vk::SampleCountFlags,
) -> Result<()> {
    if !encoder
        .owner
        .enabled_features
        .contains(Features::MULTISAMPLED_RENDER_TO_SINGLE_SAMPLED)
    {
        return Err(Error::Validation(
            "multisampled render-to-single-sampled requires its enabled Device feature".into(),
        ));
    }
    if count == SampleCount::One || attachment_sample_count != vk::SampleCountFlags::_1 {
        return Err(Error::Validation(
            "multisampled render-to-single-sampled requires a multisampled raster count and single-sampled attachments"
                .into(),
        ));
    }
    if !encoder
        .owner
        .properties
        .framebuffer_color_sample_counts
        .contains(count.as_supported_set())
    {
        return Err(Error::Validation(format!(
            "multisampled render-to-single-sampled count {count:?} is unsupported by the Device"
        )));
    }
    let has_explicit_resolve = descriptor
        .color_attachments
        .iter()
        .flatten()
        .any(|attachment| attachment.resolve_target.is_some())
        || descriptor
            .depth_attachment
            .is_some_and(|attachment| attachment.resolve_target.is_some())
        || descriptor
            .stencil_attachment
            .is_some_and(|attachment| attachment.resolve_target.is_some());
    if has_explicit_resolve {
        return Err(Error::Validation(
            "multisampled render-to-single-sampled cannot be combined with explicit resolve attachments"
                .into(),
        ));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_attachment(
    encoder: &CommandEncoder,
    view: AttachmentView<'_>,
    layout: crate::TextureLayout,
    resolve_target: Option<AttachmentView<'_>>,
    resolve_layout: crate::TextureLayout,
    resolve_mode: ResolveMode,
    sample_count: &mut Option<vk::SampleCountFlags>,
) -> Result<()> {
    if !view.belongs_to(&encoder.owner) {
        return Err(Error::Validation(
            "rendering attachment was created by a different Device".into(),
        ));
    }
    validate_attachment_layout(layout)?;
    match *sample_count {
        Some(count) if count != view.sample_count() => {
            return Err(Error::Validation(
                "all non-resolve rendering attachments must use the same sample count".into(),
            ));
        }
        None => *sample_count = Some(view.sample_count()),
        _ => {}
    }
    match resolve_target {
        Some(resolve_target) => {
            if !resolve_target.belongs_to(&encoder.owner) {
                return Err(Error::Validation(
                    "resolve attachment was created by a different Device".into(),
                ));
            }
            if resolve_mode == ResolveMode::None {
                return Err(Error::Validation(
                    "resolve attachment requires exactly one resolve mode".into(),
                ));
            }
            if view.sample_count() == vk::SampleCountFlags::_1
                || resolve_target.sample_count() != vk::SampleCountFlags::_1
            {
                return Err(Error::Validation(
                    "resolve requires a multisampled source and single-sampled target".into(),
                ));
            }
            if view.format() != resolve_target.format() {
                return Err(Error::Validation(
                    "resolve source and target formats must match".into(),
                ));
            }
            validate_attachment_layout(resolve_layout)?;
        }
        None if resolve_mode != ResolveMode::None => {
            return Err(Error::Validation(
                "resolve mode must be empty when no resolve target is provided".into(),
            ));
        }
        None => {}
    }
    Ok(())
}

fn validate_attachment_layout(layout: crate::TextureLayout) -> Result<()> {
    if layout == crate::TextureLayout::Undefined {
        return Err(Error::Validation(
            "rendering attachment layout must preserve defined image contents".into(),
        ));
    }
    Ok(())
}
