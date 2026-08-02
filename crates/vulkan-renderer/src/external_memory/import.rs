use std::os::fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, IntoRawFd, OwnedFd};
use std::sync::Arc;

use vulkanalia::{
    prelude::v1_4::*,
    vk::{self, KhrExternalMemoryFdExtensionDeviceCommands},
};

use crate::backend::DeviceOwner;
use crate::{Error, Result};

use super::{
    DmaBufImageDescriptor, ImportedDmaBufImage, ImportedDmaBufImageInner,
    choose_import_memory_type, color_subresource_range,
};

pub(super) fn finish_imported_image(
    owner: Arc<DeviceOwner>,
    image: vk::Image,
    memories: Vec<vk::DeviceMemory>,
    descriptor: &DmaBufImageDescriptor,
) -> Result<ImportedDmaBufImage> {
    let view_info = vk::ImageViewCreateInfo::builder()
        .image(image)
        .view_type(vk::ImageViewType::_2D)
        .format(descriptor.format.to_vk())
        .components(descriptor.components.to_vk())
        .subresource_range(color_subresource_range().to_vk());
    let view = match unsafe { owner.device.create_image_view(&view_info, None) } {
        Ok(view) => view,
        Err(source) => {
            unsafe {
                owner.device.destroy_image(image, None);
                for memory in &memories {
                    owner.device.free_memory(*memory, None);
                }
            }
            return Err(Error::vulkan("vkCreateImageView(import dma-buf)", source));
        }
    };
    Ok(ImportedDmaBufImage {
        inner: Arc::new(ImportedDmaBufImageInner {
            owner,
            image,
            memories,
            view,
            format: descriptor.format,
            extent: descriptor.extent,
            modifier: descriptor.modifier,
            usage: descriptor.usage,
            components: descriptor.components,
            label: descriptor.label.clone(),
        }),
    })
}

pub(super) fn create_import_image(
    owner: &Arc<DeviceOwner>,
    descriptor: &DmaBufImageDescriptor,
    disjoint: bool,
) -> Result<vk::Image> {
    let layouts = descriptor
        .planes
        .iter()
        .map(|plane| vk::SubresourceLayout {
            offset: plane.offset,
            size: 0,
            row_pitch: plane.row_pitch,
            array_pitch: 0,
            depth_pitch: 0,
        })
        .collect::<Vec<_>>();
    let mut modifier = vk::ImageDrmFormatModifierExplicitCreateInfoEXT::builder()
        .drm_format_modifier(descriptor.modifier)
        .plane_layouts(&layouts);
    let mut external = vk::ExternalMemoryImageCreateInfo::builder()
        .handle_types(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);
    let create = vk::ImageCreateInfo::builder()
        .flags(if disjoint {
            vk::ImageCreateFlags::DISJOINT
        } else {
            vk::ImageCreateFlags::empty()
        })
        .image_type(vk::ImageType::_2D)
        .format(descriptor.format.to_vk())
        .extent(vk::Extent3D {
            width: descriptor.extent.width,
            height: descriptor.extent.height,
            depth: 1,
        })
        .mip_levels(1)
        .array_layers(1)
        .samples(vk::SampleCountFlags::_1)
        .tiling(vk::ImageTiling::DRM_FORMAT_MODIFIER_EXT)
        .usage(descriptor.usage.to_vk())
        .sharing_mode(vk::SharingMode::EXCLUSIVE)
        .initial_layout(vk::ImageLayout::UNDEFINED)
        .push_next(&mut modifier)
        .push_next(&mut external);
    unsafe { owner.device.create_image(&create, None) }
        .map_err(|source| Error::vulkan("vkCreateImage(import dma-buf)", source))
}

pub(super) fn allocate_import_memory(
    owner: &Arc<DeviceOwner>,
    image: vk::Image,
    fd: impl AsFd,
) -> Result<vk::DeviceMemory> {
    let requirements = unsafe { owner.device.get_image_memory_requirements(image) };
    let memory = allocate_imported_memory(owner, image, fd.as_fd(), requirements)?;
    if let Err(source) = unsafe { owner.device.bind_image_memory(image, memory, 0) } {
        unsafe { owner.device.free_memory(memory, None) };
        return Err(Error::vulkan("vkBindImageMemory(import dma-buf)", source));
    }
    Ok(memory)
}

pub(super) fn allocate_disjoint_import_memory(
    owner: &Arc<DeviceOwner>,
    image: vk::Image,
    plane_fds: &[BorrowedFd<'_>],
) -> Result<Vec<vk::DeviceMemory>> {
    let mut memories = Vec::with_capacity(plane_fds.len());
    for (plane, fd) in plane_fds.iter().copied().enumerate() {
        let aspect = memory_plane_aspect(plane);
        let requirements = image_plane_memory_requirements(owner, image, aspect);
        match allocate_imported_memory(owner, image, fd, requirements) {
            Ok(memory) => memories.push(memory),
            Err(error) => {
                unsafe {
                    for memory in &memories {
                        owner.device.free_memory(*memory, None);
                    }
                }
                return Err(error);
            }
        }
    }

    let mut plane_infos = (0..plane_fds.len())
        .map(|plane| {
            vk::BindImagePlaneMemoryInfo::builder()
                .plane_aspect(memory_plane_aspect(plane))
                .build()
        })
        .collect::<Vec<_>>();
    let binds = plane_infos
        .iter_mut()
        .zip(&memories)
        .map(|(plane, memory)| {
            vk::BindImageMemoryInfo::builder()
                .image(image)
                .memory(*memory)
                .memory_offset(0)
                .push_next(plane)
                .build()
        })
        .collect::<Vec<_>>();
    if let Err(source) = unsafe { owner.device.bind_image_memory2(&binds) } {
        unsafe {
            for memory in &memories {
                owner.device.free_memory(*memory, None);
            }
        }
        return Err(Error::vulkan(
            "vkBindImageMemory2(import disjoint dma-buf)",
            source,
        ));
    }
    Ok(memories)
}

fn image_plane_memory_requirements(
    owner: &Arc<DeviceOwner>,
    image: vk::Image,
    aspect: vk::ImageAspectFlags,
) -> vk::MemoryRequirements {
    let mut plane = vk::ImagePlaneMemoryRequirementsInfo::builder()
        .plane_aspect(aspect)
        .build();
    let info = vk::ImageMemoryRequirementsInfo2::builder()
        .image(image)
        .push_next(&mut plane);
    let mut requirements = vk::MemoryRequirements2::default();
    unsafe {
        owner
            .device
            .get_image_memory_requirements2(&info, &mut requirements)
    };
    requirements.memory_requirements
}

fn allocate_imported_memory(
    owner: &Arc<DeviceOwner>,
    image: vk::Image,
    fd: BorrowedFd<'_>,
    requirements: vk::MemoryRequirements,
) -> Result<vk::DeviceMemory> {
    let mut fd_properties = vk::MemoryFdPropertiesKHR::default();
    unsafe {
        owner.device.get_memory_fd_properties_khr(
            vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT,
            fd.as_raw_fd(),
            &mut fd_properties,
        )
    }
    .map_err(|source| Error::vulkan("vkGetMemoryFdPropertiesKHR(DMA_BUF_EXT)", source))?;
    let memory_type_bits = requirements.memory_type_bits & fd_properties.memory_type_bits;
    let memory_type_index = choose_import_memory_type(
        &owner.instance_owner().instance,
        owner.physical_device(),
        memory_type_bits,
        requirements.size,
    )?;
    let duplicate = fd
        .try_clone_to_owned()
        .map_err(|error| Error::Validation(format!("duplicate dma-buf fd: {error}")))?;
    let raw_fd = duplicate.into_raw_fd();
    let mut import = vk::ImportMemoryFdInfoKHR::builder()
        .handle_type(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT)
        .fd(raw_fd);
    let mut dedicated = vk::MemoryDedicatedAllocateInfo::builder().image(image);
    let allocate = vk::MemoryAllocateInfo::builder()
        .allocation_size(requirements.size)
        .memory_type_index(memory_type_index)
        .push_next(&mut import)
        .push_next(&mut dedicated);
    let memory = match unsafe { owner.device.allocate_memory(&allocate, None) } {
        Ok(memory) => memory,
        Err(source) => {
            unsafe { drop(OwnedFd::from_raw_fd(raw_fd)) };
            return Err(Error::vulkan(
                "vkAllocateMemory(import DMA_BUF_EXT)",
                source,
            ));
        }
    };
    Ok(memory)
}

pub(super) const fn memory_plane_aspect(plane: usize) -> vk::ImageAspectFlags {
    match plane {
        0 => vk::ImageAspectFlags::MEMORY_PLANE_0_EXT,
        1 => vk::ImageAspectFlags::MEMORY_PLANE_1_EXT,
        2 => vk::ImageAspectFlags::MEMORY_PLANE_2_EXT,
        3 => vk::ImageAspectFlags::MEMORY_PLANE_3_EXT,
        _ => unreachable!(),
    }
}
