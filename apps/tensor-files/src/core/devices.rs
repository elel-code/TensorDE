use gio::prelude::*;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc::Sender;

mod actions;

pub use actions::{
    DevicePlaceOperation, DevicePlaceOperationResult, perform_device_place_operation,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceInfo {
    pub id: String,
    pub mount_point: Option<PathBuf>,
    pub uri: Option<String>,
    pub filesystem_type: Option<String>,
    pub label: Option<String>,
    pub capacity_bytes: Option<u64>,
    pub removable: bool,
    pub mounted: bool,
    pub ejectable: bool,
    pub can_power_off: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DeviceEvent {
    Added(DeviceInfo),
    Removed(String),
    Changed(DeviceInfo),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DeviceMonitorMessage {
    Snapshot(Vec<DeviceInfo>),
    Events {
        events: Vec<DeviceEvent>,
        devices: Vec<DeviceInfo>,
    },
}

#[derive(Clone, Debug)]
pub enum DeviceDiscoveryError {
    Gio(String),
}

impl fmt::Display for DeviceDiscoveryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Gio(message) => write!(f, "{message}"),
        }
    }
}

impl Error for DeviceDiscoveryError {}

#[derive(Clone, Debug)]
pub enum DeviceActionError {
    Discovery(DeviceDiscoveryError),
    DeviceNotFound(String),
    NotMounted(String),
    MissingMountPoint(String),
    CannotEject(String),
    CannotPowerOff(String),
    Gio(String),
}

impl fmt::Display for DeviceActionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Discovery(error) => write!(f, "{error}"),
            Self::DeviceNotFound(id) => write!(f, "no GIO device matches {id}"),
            Self::NotMounted(id) => write!(f, "device is not mounted: {id}"),
            Self::MissingMountPoint(id) => write!(f, "device has no local mount point: {id}"),
            Self::CannotEject(id) => write!(f, "device cannot be ejected: {id}"),
            Self::CannotPowerOff(id) => write!(f, "device cannot be safely removed: {id}"),
            Self::Gio(message) => write!(f, "{message}"),
        }
    }
}

impl Error for DeviceActionError {}

impl From<DeviceDiscoveryError> for DeviceActionError {
    fn from(error: DeviceDiscoveryError) -> Self {
        Self::Discovery(error)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceMountResult {
    pub device_id: String,
    pub mount_point: PathBuf,
}

enum GioDeviceHandle {
    Mount(gio::Mount, DeviceInfo),
    Volume(gio::Volume, DeviceInfo),
}

pub async fn read_devices() -> Result<Vec<DeviceInfo>, DeviceDiscoveryError> {
    read_gio_devices()
}

pub fn read_gio_devices() -> Result<Vec<DeviceInfo>, DeviceDiscoveryError> {
    let monitor = gio::VolumeMonitor::get();
    Ok(devices_from_gio_monitor(&monitor))
}

pub async fn watch_devices(
    sender: Sender<DeviceMonitorMessage>,
) -> Result<(), DeviceDiscoveryError> {
    watch_gio_devices_blocking(sender)
}

fn watch_gio_devices_blocking(
    sender: Sender<DeviceMonitorMessage>,
) -> Result<(), DeviceDiscoveryError> {
    let main_loop = gio::glib::MainLoop::new(None, false);
    let monitor = gio::VolumeMonitor::get();

    if !send_gio_snapshot(&sender, &monitor) {
        return Ok(());
    }

    {
        let sender = sender.clone();
        let signal_monitor = monitor.clone();
        let snapshot_monitor = monitor.clone();
        let main_loop = main_loop.clone();
        signal_monitor.connect_mount_changed(move |_, _| {
            if !send_gio_snapshot(&sender, &snapshot_monitor) {
                main_loop.quit();
            }
        });
    }
    {
        let sender = sender.clone();
        let signal_monitor = monitor.clone();
        let snapshot_monitor = monitor.clone();
        let main_loop = main_loop.clone();
        signal_monitor.connect_mount_added(move |_, _| {
            if !send_gio_snapshot(&sender, &snapshot_monitor) {
                main_loop.quit();
            }
        });
    }
    {
        let sender = sender.clone();
        let signal_monitor = monitor.clone();
        let snapshot_monitor = monitor.clone();
        let main_loop = main_loop.clone();
        signal_monitor.connect_mount_removed(move |_, _| {
            if !send_gio_snapshot(&sender, &snapshot_monitor) {
                main_loop.quit();
            }
        });
    }
    {
        let sender = sender.clone();
        let signal_monitor = monitor.clone();
        let snapshot_monitor = monitor.clone();
        let main_loop = main_loop.clone();
        signal_monitor.connect_volume_changed(move |_, _| {
            if !send_gio_snapshot(&sender, &snapshot_monitor) {
                main_loop.quit();
            }
        });
    }
    {
        let sender = sender.clone();
        let signal_monitor = monitor.clone();
        let snapshot_monitor = monitor.clone();
        let main_loop = main_loop.clone();
        signal_monitor.connect_volume_added(move |_, _| {
            if !send_gio_snapshot(&sender, &snapshot_monitor) {
                main_loop.quit();
            }
        });
    }
    {
        let sender = sender.clone();
        let signal_monitor = monitor.clone();
        let snapshot_monitor = monitor.clone();
        let main_loop = main_loop.clone();
        signal_monitor.connect_volume_removed(move |_, _| {
            if !send_gio_snapshot(&sender, &snapshot_monitor) {
                main_loop.quit();
            }
        });
    }

    main_loop.run();
    Ok(())
}

fn send_gio_snapshot(sender: &Sender<DeviceMonitorMessage>, monitor: &gio::VolumeMonitor) -> bool {
    sender
        .send(DeviceMonitorMessage::Snapshot(devices_from_gio_monitor(
            monitor,
        )))
        .is_ok()
}

pub async fn mount_device(device_id: &str) -> Result<DeviceMountResult, DeviceActionError> {
    mount_gio_device(device_id)
}

pub async fn unmount_device(device_id: &str) -> Result<(), DeviceActionError> {
    unmount_gio_device(device_id)
}

pub async fn eject_device(device_id: &str) -> Result<(), DeviceActionError> {
    eject_gio_device(device_id)
}

pub async fn safely_remove_device(device_id: &str) -> Result<(), DeviceActionError> {
    let handle = resolve_gio_device_handle(device_id)
        .ok_or_else(|| DeviceActionError::DeviceNotFound(device_id.to_string()))?;
    let can_power_off = match &handle {
        GioDeviceHandle::Mount(_, info) | GioDeviceHandle::Volume(_, info) => info.can_power_off,
    };
    if !can_power_off {
        return Err(DeviceActionError::CannotPowerOff(device_id.to_string()));
    }
    eject_gio_device(device_id)
}

fn mount_gio_device(device_id: &str) -> Result<DeviceMountResult, DeviceActionError> {
    match resolve_gio_device_handle(device_id)
        .ok_or_else(|| DeviceActionError::DeviceNotFound(device_id.to_string()))?
    {
        GioDeviceHandle::Mount(_, info) => {
            let mount_point = info
                .mount_point
                .clone()
                .ok_or_else(|| DeviceActionError::MissingMountPoint(device_id.to_string()))?;
            Ok(DeviceMountResult {
                device_id: info.id,
                mount_point,
            })
        }
        GioDeviceHandle::Volume(volume, info) => {
            run_volume_mount(&volume, device_id)?;
            let mount = volume
                .get_mount()
                .ok_or_else(|| DeviceActionError::MissingMountPoint(device_id.to_string()))?;
            let mount_point = MountExt::root(&mount)
                .path()
                .ok_or_else(|| DeviceActionError::MissingMountPoint(device_id.to_string()))?;
            Ok(DeviceMountResult {
                device_id: info.id,
                mount_point,
            })
        }
    }
}

fn unmount_gio_device(device_id: &str) -> Result<(), DeviceActionError> {
    match resolve_gio_device_handle(device_id)
        .ok_or_else(|| DeviceActionError::DeviceNotFound(device_id.to_string()))?
    {
        GioDeviceHandle::Mount(mount, _) => {
            if MountExt::can_unmount(&mount) {
                run_mount_unmount(&mount, device_id)
            } else if MountExt::can_eject(&mount) {
                run_mount_eject(&mount, device_id)
            } else {
                Err(DeviceActionError::NotMounted(device_id.to_string()))
            }
        }
        GioDeviceHandle::Volume(_, _) => Err(DeviceActionError::NotMounted(device_id.to_string())),
    }
}

fn eject_gio_device(device_id: &str) -> Result<(), DeviceActionError> {
    match resolve_gio_device_handle(device_id)
        .ok_or_else(|| DeviceActionError::DeviceNotFound(device_id.to_string()))?
    {
        GioDeviceHandle::Mount(mount, _) if MountExt::can_eject(&mount) => {
            run_mount_eject(&mount, device_id)
        }
        GioDeviceHandle::Volume(volume, _) if VolumeExt::can_eject(&volume) => {
            run_volume_eject(&volume, device_id)
        }
        GioDeviceHandle::Mount(_, _) | GioDeviceHandle::Volume(_, _) => {
            Err(DeviceActionError::CannotEject(device_id.to_string()))
        }
    }
}

fn resolve_gio_device_handle(device_id: &str) -> Option<GioDeviceHandle> {
    let monitor = gio::VolumeMonitor::get();
    for (index, mount) in monitor.mounts().into_iter().enumerate() {
        if MountExt::is_shadowed(&mount) {
            continue;
        }
        let info = device_info_from_mount(&mount, index)?;
        if info.id == device_id {
            return Some(GioDeviceHandle::Mount(mount, info));
        }
    }
    for (index, volume) in monitor.volumes().into_iter().enumerate() {
        if volume.get_mount().is_some() {
            continue;
        }
        let info = device_info_from_volume(&volume, index)?;
        if info.id == device_id {
            return Some(GioDeviceHandle::Volume(volume, info));
        }
    }
    None
}

fn devices_from_gio_monitor(monitor: &gio::VolumeMonitor) -> Vec<DeviceInfo> {
    let mut devices = BTreeMap::<String, DeviceInfo>::new();
    for (index, mount) in monitor.mounts().into_iter().enumerate() {
        if MountExt::is_shadowed(&mount) {
            continue;
        }
        if let Some(info) = device_info_from_mount(&mount, index) {
            devices.insert(info.id.clone(), info);
        }
    }
    for (index, volume) in monitor.volumes().into_iter().enumerate() {
        if volume.get_mount().is_some() {
            continue;
        }
        if let Some(info) = device_info_from_volume(&volume, index) {
            devices.insert(info.id.clone(), info);
        }
    }
    devices.into_values().collect()
}

fn device_info_from_mount(mount: &gio::Mount, index: usize) -> Option<DeviceInfo> {
    let root = MountExt::root(mount);
    let mount_point = root.path();
    let remote = file_is_remote(&root);
    if remote && mount_point.is_none() {
        return None;
    }
    let (filesystem_type, capacity_bytes) = filesystem_metadata(&root);
    let label = string_value(MountExt::name(mount).as_str())
        .or_else(|| mount_point.as_deref().and_then(label_from_path));
    Some(DeviceInfo {
        id: mount_device_id(mount, index),
        mount_point,
        uri: string_value(root.uri().as_str()),
        filesystem_type,
        label,
        capacity_bytes,
        removable: true,
        mounted: true,
        ejectable: MountExt::can_eject(mount),
        can_power_off: false,
    })
}

fn device_info_from_volume(volume: &gio::Volume, index: usize) -> Option<DeviceInfo> {
    if !VolumeExt::can_mount(volume) && !VolumeExt::can_eject(volume) {
        return None;
    }
    let activation_root = VolumeExt::activation_root(volume);
    Some(DeviceInfo {
        id: volume_device_id(volume, index),
        mount_point: None,
        uri: activation_root
            .as_ref()
            .and_then(|file| string_value(file.uri().as_str())),
        filesystem_type: None,
        label: string_value(VolumeExt::name(volume).as_str()),
        capacity_bytes: None,
        removable: true,
        mounted: false,
        ejectable: VolumeExt::can_eject(volume),
        can_power_off: false,
    })
}

fn mount_device_id(mount: &gio::Mount, index: usize) -> String {
    if let Some(uuid) = MountExt::uuid(mount).and_then(|uuid| string_value(uuid.as_str())) {
        return format!("gio:mount:uuid:{uuid}");
    }
    let root = MountExt::root(mount);
    if let Some(uri) = string_value(root.uri().as_str()) {
        return format!("gio:mount:uri:{uri}");
    }
    let name = MountExt::name(mount);
    format!("gio:mount:name:{name}:{index}")
}

fn volume_device_id(volume: &gio::Volume, index: usize) -> String {
    if let Some(uuid) = VolumeExt::uuid(volume).and_then(|uuid| string_value(uuid.as_str())) {
        return format!("gio:volume:uuid:{uuid}");
    }
    if let Some(root) = VolumeExt::activation_root(volume)
        && let Some(uri) = string_value(root.uri().as_str())
    {
        return format!("gio:volume:uri:{uri}");
    }
    let name = VolumeExt::name(volume);
    format!("gio:volume:name:{name}:{index}")
}

fn filesystem_metadata(root: &gio::File) -> (Option<String>, Option<u64>) {
    let Ok(info) =
        root.query_filesystem_info("filesystem::type,filesystem::size", gio::Cancellable::NONE)
    else {
        return (None, None);
    };
    let filesystem_type = info
        .attribute_as_string("filesystem::type")
        .and_then(|value| string_value(value.as_str()));
    let capacity_bytes = info
        .attribute_uint64("filesystem::size")
        .checked_sub(0)
        .filter(|size| *size > 0);
    (filesystem_type, capacity_bytes)
}

fn file_is_remote(file: &gio::File) -> bool {
    file.query_filesystem_info("filesystem::remote", gio::Cancellable::NONE)
        .ok()
        .map(|info| info.boolean("filesystem::remote"))
        .unwrap_or(false)
}

fn run_volume_mount(volume: &gio::Volume, device_id: &str) -> Result<(), DeviceActionError> {
    let main_loop = gio::glib::MainLoop::new(None, false);
    let result = Rc::new(RefCell::new(None));
    let result_for_callback = result.clone();
    let loop_for_callback = main_loop.clone();
    VolumeExt::mount(
        volume,
        gio::MountMountFlags::NONE,
        gio::MountOperation::NONE,
        gio::Cancellable::NONE,
        move |res| {
            *result_for_callback.borrow_mut() = Some(res);
            loop_for_callback.quit();
        },
    );
    wait_for_gio_result(main_loop, result, "mount", device_id)
}

fn run_mount_unmount(mount: &gio::Mount, device_id: &str) -> Result<(), DeviceActionError> {
    let main_loop = gio::glib::MainLoop::new(None, false);
    let result = Rc::new(RefCell::new(None));
    let result_for_callback = result.clone();
    let loop_for_callback = main_loop.clone();
    MountExt::unmount_with_operation(
        mount,
        gio::MountUnmountFlags::NONE,
        gio::MountOperation::NONE,
        gio::Cancellable::NONE,
        move |res| {
            *result_for_callback.borrow_mut() = Some(res);
            loop_for_callback.quit();
        },
    );
    wait_for_gio_result(main_loop, result, "unmount", device_id)
}

fn run_mount_eject(mount: &gio::Mount, device_id: &str) -> Result<(), DeviceActionError> {
    let main_loop = gio::glib::MainLoop::new(None, false);
    let result = Rc::new(RefCell::new(None));
    let result_for_callback = result.clone();
    let loop_for_callback = main_loop.clone();
    MountExt::eject_with_operation(
        mount,
        gio::MountUnmountFlags::NONE,
        gio::MountOperation::NONE,
        gio::Cancellable::NONE,
        move |res| {
            *result_for_callback.borrow_mut() = Some(res);
            loop_for_callback.quit();
        },
    );
    wait_for_gio_result(main_loop, result, "eject", device_id)
}

fn run_volume_eject(volume: &gio::Volume, device_id: &str) -> Result<(), DeviceActionError> {
    let main_loop = gio::glib::MainLoop::new(None, false);
    let result = Rc::new(RefCell::new(None));
    let result_for_callback = result.clone();
    let loop_for_callback = main_loop.clone();
    VolumeExt::eject_with_operation(
        volume,
        gio::MountUnmountFlags::NONE,
        gio::MountOperation::NONE,
        gio::Cancellable::NONE,
        move |res| {
            *result_for_callback.borrow_mut() = Some(res);
            loop_for_callback.quit();
        },
    );
    wait_for_gio_result(main_loop, result, "eject", device_id)
}

fn wait_for_gio_result(
    main_loop: gio::glib::MainLoop,
    result: Rc<RefCell<Option<Result<(), gio::glib::Error>>>>,
    operation: &str,
    device_id: &str,
) -> Result<(), DeviceActionError> {
    main_loop.run();
    let result = result.borrow_mut().take().ok_or_else(|| {
        DeviceActionError::Gio(format!(
            "GIO {operation} finished without a result for {device_id}"
        ))
    })?;
    result.map_err(|error| DeviceActionError::Gio(format!("GIO {operation} failed: {error}")))
}

pub fn device_events_between(previous: &[DeviceInfo], current: &[DeviceInfo]) -> Vec<DeviceEvent> {
    let previous = previous
        .iter()
        .map(|device| (device.id.clone(), device))
        .collect::<BTreeMap<_, _>>();
    let current = current
        .iter()
        .map(|device| (device.id.clone(), device))
        .collect::<BTreeMap<_, _>>();
    let mut events = Vec::new();
    for device_id in previous.keys() {
        if !current.contains_key(device_id) {
            events.push(DeviceEvent::Removed(device_id.clone()));
        }
    }
    for (device_id, device) in current {
        match previous.get(&device_id) {
            None => events.push(DeviceEvent::Added(device.clone())),
            Some(previous) if **previous != *device => {
                events.push(DeviceEvent::Changed(device.clone()));
            }
            Some(_) => {}
        }
    }
    events
}

fn string_value(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn label_from_path(path: &std::path::Path) -> Option<String> {
    path.file_name()
        .and_then(|name| name.to_str())
        .and_then(string_value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_device(id: &str, label: &str, mounted: bool) -> DeviceInfo {
        DeviceInfo {
            id: id.to_string(),
            mount_point: mounted.then(|| PathBuf::from(format!("/run/media/{label}"))),
            uri: mounted.then(|| format!("file:///run/media/{label}")),
            filesystem_type: mounted.then(|| "exfat".to_string()),
            label: Some(label.to_string()),
            capacity_bytes: Some(1024),
            removable: true,
            mounted,
            ejectable: mounted,
            can_power_off: false,
        }
    }

    #[test]
    fn device_events_report_added_removed_and_changed_devices_by_id() {
        let previous = vec![
            test_device("gio:mount:uri:file:///old", "Old", true),
            test_device("gio:mount:uri:file:///removed", "Removed", true),
        ];
        let mut changed = test_device("gio:mount:uri:file:///old", "New", true);
        changed.mount_point = Some(PathBuf::from("/run/media/New"));
        let current = vec![
            changed.clone(),
            test_device("gio:volume:uuid:added", "Added", false),
        ];

        assert_eq!(
            device_events_between(&previous, &current),
            vec![
                DeviceEvent::Removed("gio:mount:uri:file:///removed".to_string()),
                DeviceEvent::Changed(changed),
                DeviceEvent::Added(test_device("gio:volume:uuid:added", "Added", false)),
            ]
        );
    }
}
