use std::hash::{Hash, Hasher};

use bytemuck::Pod;

use crate::ui::render::quad::QuadVertex;
use crate::ui::render::texture::TextVertex;

pub(crate) fn create_vertex_buffer(device: &wgpu::Device, vertex_capacity: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("fika-wgpu-quad-vertices"),
        size: (vertex_capacity.max(1) * std::mem::size_of::<QuadVertex>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct VertexBufferUploadStats {
    pub(crate) writes: usize,
    pub(crate) skips: usize,
}

impl VertexBufferUploadStats {
    pub(crate) fn merge(&mut self, other: Self) {
        self.writes += other.writes;
        self.skips += other.skips;
    }
}

pub(crate) fn upload_vertex_buffer_if_dirty<T: Pod>(
    queue: &wgpu::Queue,
    vertex_buffer: &wgpu::Buffer,
    vertices: &[T],
    last_hash: &mut Option<u64>,
) -> VertexBufferUploadStats {
    let Some(hash) = vertex_slice_hash(vertices) else {
        *last_hash = None;
        return VertexBufferUploadStats::default();
    };
    if *last_hash == Some(hash) {
        return VertexBufferUploadStats {
            writes: 0,
            skips: 1,
        };
    }
    queue.write_buffer(vertex_buffer, 0, bytemuck::cast_slice(vertices));
    *last_hash = Some(hash);
    VertexBufferUploadStats {
        writes: 1,
        skips: 0,
    }
}

#[cfg(test)]
pub(crate) fn upload_vertex_hash_for_test<T: Pod>(
    vertices: &[T],
    last_hash: &mut Option<u64>,
) -> VertexBufferUploadStats {
    let Some(hash) = vertex_slice_hash(vertices) else {
        *last_hash = None;
        return VertexBufferUploadStats::default();
    };
    if *last_hash == Some(hash) {
        return VertexBufferUploadStats {
            writes: 0,
            skips: 1,
        };
    }
    *last_hash = Some(hash);
    VertexBufferUploadStats {
        writes: 1,
        skips: 0,
    }
}

fn vertex_slice_hash<T: Pod>(vertices: &[T]) -> Option<u64> {
    if vertices.is_empty() {
        return None;
    }
    Some(hash_bytes_with_len(bytemuck::cast_slice(vertices)))
}

pub(crate) fn vertex_pair_hash<T: Pod>(first: &[T], second: &[T]) -> Option<u64> {
    if first.is_empty() && second.is_empty() {
        return None;
    }
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    first.len().hash(&mut hasher);
    bytemuck::cast_slice::<T, u8>(first).hash(&mut hasher);
    second.len().hash(&mut hasher);
    bytemuck::cast_slice::<T, u8>(second).hash(&mut hasher);
    Some(hasher.finish())
}

pub(crate) fn hash_bytes_with_len(bytes: &[u8]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.len().hash(&mut hasher);
    bytes.hash(&mut hasher);
    hasher.finish()
}

pub(crate) fn create_text_vertex_buffer(
    device: &wgpu::Device,
    vertex_capacity: usize,
) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("fika-wgpu-text-vertices"),
        size: (vertex_capacity.max(1) * std::mem::size_of::<TextVertex>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

pub(crate) fn create_text_texture(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("fika-wgpu-text-atlas"),
        size: wgpu::Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    })
}

pub(crate) fn create_text_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    texture_view: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("fika-wgpu-text-bind-group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(texture_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}

pub(crate) fn create_icon_texture(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("fika-wgpu-icon-atlas"),
        size: wgpu::Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    })
}

pub(crate) fn create_icon_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    texture_view: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("fika-wgpu-icon-bind-group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(texture_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}
