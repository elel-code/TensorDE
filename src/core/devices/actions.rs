use super::{DeviceActionError, eject_device, mount_device, safely_remove_device, unmount_device};
use crate::core::pane::PaneId;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DevicePlaceOperation {
    Mount,
    Unmount,
    Eject,
    SafelyRemove,
}

impl DevicePlaceOperation {
    pub fn in_progress_message(self, label: &str) -> String {
        match self {
            Self::Mount => format!("Mounting {label}"),
            Self::Unmount => format!("Unmounting {label}"),
            Self::Eject => format!("Ejecting {label}"),
            Self::SafelyRemove => format!("Safely removing {label}"),
        }
    }

    pub fn success_message(self, label: &str) -> String {
        match self {
            Self::Mount => format!("Mounted {label}"),
            Self::Unmount => format!("Unmounted {label}"),
            Self::Eject => format!("Ejected {label}"),
            Self::SafelyRemove => format!("Safely removed {label}"),
        }
    }

    pub fn error_message(self, label: &str, error: &DeviceActionError) -> String {
        match self {
            Self::Mount => format!("Cannot mount {label}: {error}"),
            Self::Unmount => format!("Cannot unmount {label}: {error}"),
            Self::Eject => format!("Cannot eject {label}: {error}"),
            Self::SafelyRemove => format!("Cannot safely remove {label}: {error}"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct DevicePlaceOperationResult {
    pub pane_id: PaneId,
    pub device_id: String,
    pub label: String,
    pub operation: DevicePlaceOperation,
    pub result: Result<Option<PathBuf>, DeviceActionError>,
}

pub async fn perform_device_place_operation(
    pane_id: PaneId,
    device_id: String,
    label: String,
    operation: DevicePlaceOperation,
) -> DevicePlaceOperationResult {
    let result = match operation {
        DevicePlaceOperation::Mount => mount_device(&device_id)
            .await
            .map(|result| Some(result.mount_point)),
        DevicePlaceOperation::Unmount => unmount_device(&device_id).await.map(|()| None),
        DevicePlaceOperation::Eject => eject_device(&device_id).await.map(|()| None),
        DevicePlaceOperation::SafelyRemove => safely_remove_device(&device_id).await.map(|()| None),
    };
    DevicePlaceOperationResult {
        pane_id,
        device_id,
        label,
        operation,
        result,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_place_operation_messages_match_file_manager_actions() {
        let label = "USB";
        assert_eq!(
            DevicePlaceOperation::Mount.in_progress_message(label),
            "Mounting USB"
        );
        assert_eq!(
            DevicePlaceOperation::Unmount.success_message(label),
            "Unmounted USB"
        );
        assert_eq!(
            DevicePlaceOperation::Eject
                .error_message(label, &DeviceActionError::CannotEject("usb".to_string())),
            "Cannot eject USB: device cannot be ejected: usb"
        );
        assert_eq!(
            DevicePlaceOperation::SafelyRemove.success_message(label),
            "Safely removed USB"
        );
    }
}
