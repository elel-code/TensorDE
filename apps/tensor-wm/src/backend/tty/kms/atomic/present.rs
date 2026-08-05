//! Allocation-free steady-state atomic presentation requests.

use std::os::fd::{AsRawFd, BorrowedFd};

use drm::control::{ResourceHandle, framebuffer, plane};

use super::PlaneProperties;

const MAX_OBJECTS: usize = 2;
const MAX_PROPERTIES: usize = 4;

/// One primary update plus the selected cursor plane's explicit disabled state.
///
/// Cursor rendering will expand the second typed object in this module; it must
/// remain part of the same atomic commit as primary scanout.
pub(super) struct PresentRequest {
    objects: [u32; MAX_OBJECTS],
    property_counts: [u32; MAX_OBJECTS],
    properties: [u32; MAX_PROPERTIES],
    values: [u64; MAX_PROPERTIES],
    object_count: usize,
    property_count: usize,
}

impl PresentRequest {
    pub(super) fn new(
        primary: plane::Handle,
        primary_properties: &PlaneProperties,
        framebuffer: framebuffer::Handle,
        fence: BorrowedFd<'_>,
        cursor: Option<(plane::Handle, PlaneProperties)>,
    ) -> Self {
        let mut request = Self {
            objects: [0; MAX_OBJECTS],
            property_counts: [0; MAX_OBJECTS],
            properties: [0; MAX_PROPERTIES],
            values: [0; MAX_PROPERTIES],
            object_count: 1,
            property_count: 2,
        };
        request.objects[0] = raw_handle(primary);
        request.property_counts[0] = 2;
        request.properties[0] = u32::from(primary_properties.framebuffer);
        request.properties[1] = u32::from(primary_properties.input_fence);
        request.values[0] = u64::from(u32::from(framebuffer));
        request.values[1] = fence.as_raw_fd() as i64 as u64;

        if let Some((cursor, properties)) = cursor {
            request.objects[1] = raw_handle(cursor);
            request.property_counts[1] = 2;
            request.properties[2] = u32::from(properties.crtc);
            request.properties[3] = u32::from(properties.framebuffer);
            request.object_count = 2;
            request.property_count = 4;
        }
        request
    }

    pub(super) fn commit(&mut self, device: BorrowedFd<'_>, flags: u32) -> std::io::Result<()> {
        drm_ffi::mode::atomic_commit(
            device,
            flags,
            &mut self.objects[..self.object_count],
            &mut self.property_counts[..self.object_count],
            &mut self.properties[..self.property_count],
            &mut self.values[..self.property_count],
        )
    }

    #[cfg(test)]
    fn objects(&self) -> &[u32] {
        &self.objects[..self.object_count]
    }

    #[cfg(test)]
    fn property_counts(&self) -> &[u32] {
        &self.property_counts[..self.object_count]
    }

    #[cfg(test)]
    fn properties(&self) -> &[u32] {
        &self.properties[..self.property_count]
    }

    #[cfg(test)]
    fn values(&self) -> &[u64] {
        &self.values[..self.property_count]
    }
}

fn raw_handle(handle: impl ResourceHandle) -> u32 {
    let handle: drm::control::RawResourceHandle = handle.into();
    u32::from(handle)
}

#[cfg(test)]
mod tests {
    use std::{num::NonZeroU32, os::fd::AsFd};

    use super::*;

    fn handle<T: From<NonZeroU32>>(raw: u32) -> T {
        NonZeroU32::new(raw).unwrap().into()
    }

    fn plane_properties(base: u32) -> PlaneProperties {
        let mut next = base;
        PlaneProperties::resolve(|_, _| {
            let property = handle(next);
            next += 1;
            Ok(property)
        })
        .unwrap()
    }

    #[test]
    fn primary_present_explicitly_disables_selected_cursor_plane() {
        let primary = plane_properties(20);
        let cursor = plane_properties(40);
        let fence = rustix::pipe::pipe().unwrap().0;
        let request = PresentRequest::new(
            handle(3),
            &primary,
            handle(7),
            fence.as_fd(),
            Some((handle(4), cursor)),
        );

        assert_eq!(request.objects(), [3, 4]);
        assert_eq!(request.property_counts(), [2, 2]);
        assert_eq!(
            request.properties(),
            [
                u32::from(primary.framebuffer),
                u32::from(primary.input_fence),
                u32::from(cursor.crtc),
                u32::from(cursor.framebuffer),
            ]
        );
        assert_eq!(request.values()[0], 7);
        assert_eq!(request.values()[1], fence.as_raw_fd() as i64 as u64);
        assert_eq!(&request.values()[2..], [0, 0]);
    }

    #[test]
    fn primary_present_without_cursor_uses_only_populated_prefixes() {
        let primary = plane_properties(20);
        let fence = rustix::pipe::pipe().unwrap().0;
        let request = PresentRequest::new(handle(3), &primary, handle(7), fence.as_fd(), None);

        assert_eq!(request.objects(), [3]);
        assert_eq!(request.property_counts(), [2]);
        assert_eq!(request.properties().len(), 2);
        assert_eq!(request.values().len(), 2);
    }
}
