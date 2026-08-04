//! Tensor-owned udev monitor drained after Compio fd completions.

use std::{
    collections::HashMap,
    ffi::OsString,
    io,
    os::fd::{AsFd, BorrowedFd},
    path::{Path, PathBuf},
};

use rustix::fs::{Dev, stat};
use udev::{Device, Enumerator, EventType, MonitorBuilder, MonitorSocket};

const MAX_EVENTS_PER_COMPLETION: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum UdevEvent {
    Added { device_id: Dev },
    Changed { device_id: Dev },
    Removed { device_id: Dev },
}

pub(super) struct UdevMonitor {
    devices: HashMap<Dev, PathBuf>,
    monitor: MonitorSocket,
    events_in_completion: usize,
}

impl UdevMonitor {
    pub(super) fn new(seat: &str) -> io::Result<Self> {
        let devices = gpus_for_seat(seat)?
            .into_iter()
            .filter_map(|device| device.devnode().map(Path::to_owned))
            .filter_map(|path| match stat(&path) {
                Ok(stat) => Some((stat.st_rdev, path)),
                Err(error) => {
                    tracing::warn!(%error, path = %path.display(), "skipping unreadable DRM device");
                    None
                }
            })
            .collect();
        let monitor = MonitorBuilder::new()?.match_subsystem("drm")?.listen()?;
        Ok(Self {
            devices,
            monitor,
            events_in_completion: 0,
        })
    }

    pub(super) fn device_list(&self) -> impl Iterator<Item = (Dev, &Path)> {
        self.devices
            .iter()
            .map(|(&device_id, path)| (device_id, path.as_path()))
    }

    pub(super) fn begin_drain(&mut self) {
        self.events_in_completion = 0;
    }

    pub(super) fn next_event(&mut self) -> Option<UdevEvent> {
        while self.events_in_completion < MAX_EVENTS_PER_COMPLETION {
            let event = self.monitor.iter().next()?;
            self.events_in_completion += 1;
            let device_id = event.devnum().map(|device_id| device_id as Dev);
            if let Some(event) = apply_device_change(
                &mut self.devices,
                event.event_type(),
                device_id,
                event.devnode(),
            ) {
                return Some(event);
            }
        }
        None
    }

    pub(super) fn take_device_path(&mut self, device_id: Dev) -> Option<PathBuf> {
        self.devices.remove(&device_id)
    }

    pub(super) fn restore_device_path(&mut self, device_id: Dev, path: PathBuf) {
        self.devices.insert(device_id, path);
    }
}

impl AsFd for UdevMonitor {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.monitor.as_fd()
    }
}

fn gpus_for_seat(seat: &str) -> io::Result<Vec<Device>> {
    let mut enumerator = Enumerator::new()?;
    enumerator.match_subsystem("drm")?;
    enumerator.match_sysname("card[0-9]*")?;
    Ok(enumerator
        .scan_devices()?
        .filter(|device| {
            device
                .property_value("ID_SEAT")
                .map(OsString::from)
                .unwrap_or_else(|| OsString::from("seat0"))
                == seat
        })
        .collect())
}

fn apply_device_change(
    devices: &mut HashMap<Dev, PathBuf>,
    event_type: EventType,
    device_id: Option<Dev>,
    path: Option<&Path>,
) -> Option<UdevEvent> {
    match event_type {
        EventType::Add => {
            let (device_id, path) = (device_id?, path?.to_owned());
            if devices.insert(device_id, path).is_none() {
                Some(UdevEvent::Added { device_id })
            } else {
                None
            }
        }
        EventType::Change => {
            let device_id = device_id?;
            devices
                .contains_key(&device_id)
                .then_some(UdevEvent::Changed { device_id })
        }
        EventType::Remove => {
            let device_id = device_id?;
            devices
                .remove(&device_id)
                .map(|_| UdevEvent::Removed { device_id })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_changes_are_value_only_and_track_known_devices() {
        let mut devices = HashMap::new();
        let path = Path::new("/dev/dri/card7");

        assert_eq!(
            apply_device_change(&mut devices, EventType::Add, Some(7), Some(path)),
            Some(UdevEvent::Added { device_id: 7 })
        );
        assert_eq!(
            apply_device_change(&mut devices, EventType::Change, Some(7), None),
            Some(UdevEvent::Changed { device_id: 7 })
        );
        assert_eq!(
            apply_device_change(&mut devices, EventType::Remove, Some(7), None),
            Some(UdevEvent::Removed { device_id: 7 })
        );
        assert_eq!(
            apply_device_change(&mut devices, EventType::Change, Some(7), None),
            None
        );
    }
}
