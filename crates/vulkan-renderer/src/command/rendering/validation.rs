use vulkanalia::vk;

use super::{AttachmentView, RenderingDescriptor};
use crate::command::CommandEncoder;
use crate::pipeline::{format_has_depth, format_has_stencil};
use crate::{Error, Result};

pub(super) struct RenderingMetadata {
    pub(super) color_formats: Vec<vk::Format>,
    pub(super) depth_format: vk::Format,
    pub(super) stencil_format: vk::Format,
    pub(super) sample_count: vk::SampleCountFlags,
}

pub(super) fn validate_descriptor(
    encoder: &CommandEncoder,
    descriptor: &RenderingDescriptor<'_>,
) -> Result<RenderingMetadata> {
    if descriptor.render_area.extent.width == 0 || descriptor.render_area.extent.height == 0 {
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
                color_formats.push(attachment.view.format());
            }
            None => color_formats.push(vk::Format::UNDEFINED),
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
    Ok(RenderingMetadata {
        color_formats,
        depth_format: descriptor
            .depth_attachment
            .map_or(vk::Format::UNDEFINED, |attachment| attachment.view.format()),
        stencil_format: descriptor
            .stencil_attachment
            .map_or(vk::Format::UNDEFINED, |attachment| attachment.view.format()),
        sample_count: sample_count.unwrap_or(vk::SampleCountFlags::_1),
    })
}

#[allow(clippy::too_many_arguments)]
fn validate_attachment(
    encoder: &CommandEncoder,
    view: AttachmentView<'_>,
    layout: vk::ImageLayout,
    resolve_target: Option<AttachmentView<'_>>,
    resolve_layout: vk::ImageLayout,
    resolve_mode: vk::ResolveModeFlags,
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
            if resolve_mode.is_empty() || resolve_mode.bits().count_ones() != 1 {
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
        None if !resolve_mode.is_empty() => {
            return Err(Error::Validation(
                "resolve mode must be empty when no resolve target is provided".into(),
            ));
        }
        None => {}
    }
    Ok(())
}

fn validate_attachment_layout(layout: vk::ImageLayout) -> Result<()> {
    if matches!(
        layout,
        vk::ImageLayout::UNDEFINED | vk::ImageLayout::PREINITIALIZED
    ) {
        return Err(Error::Validation(
            "rendering attachment layout must preserve defined image contents".into(),
        ));
    }
    Ok(())
}
