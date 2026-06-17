//! Linux power state detection through sysfs power_supply.

use super::PowerState;
use std::fs;
use std::io;
use std::path::Path;

const SYSFS_POWER_SUPPLY: &str = "/sys/class/power_supply";

pub fn read_power_state() -> PowerState {
    read_power_state_from_sysfs(SYSFS_POWER_SUPPLY).unwrap_or(PowerState::Unknown)
}

pub fn read_power_state_from_sysfs(root: impl AsRef<Path>) -> io::Result<PowerState> {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(PowerState::Unknown),
        Err(err) => return Err(err),
    };

    let mut system_battery = false;
    let mut discharging_battery = false;
    let mut externally_powered_battery = false;
    let mut online_external_supply = false;
    let mut offline_external_supply = false;

    for entry in entries {
        let path = entry?.path();
        if !path.is_dir() {
            continue;
        }

        let supply_type = read_trimmed(path.join("type"))
            .unwrap_or_default()
            .to_ascii_lowercase();
        if supply_type == "battery" {
            if !is_system_battery(&path) {
                continue;
            }
            system_battery = true;
            match read_trimmed(path.join("status"))
                .unwrap_or_default()
                .to_ascii_lowercase()
                .as_str()
            {
                "discharging" => discharging_battery = true,
                "charging" | "full" | "not charging" => externally_powered_battery = true,
                _ => {}
            }
        } else if is_external_supply(&supply_type) {
            match read_bool(path.join("online")) {
                Some(true) => online_external_supply = true,
                Some(false) => offline_external_supply = true,
                None => {}
            }
        }
    }

    if online_external_supply || externally_powered_battery {
        return Ok(PowerState::Ac);
    }
    if system_battery && (discharging_battery || offline_external_supply) {
        return Ok(PowerState::Battery);
    }
    Ok(PowerState::Unknown)
}

fn is_system_battery(path: &Path) -> bool {
    read_trimmed(path.join("scope"))
        .map(|scope| !scope.eq_ignore_ascii_case("device"))
        .unwrap_or(true)
}

fn is_external_supply(supply_type: &str) -> bool {
    matches!(
        supply_type,
        "mains"
            | "usb"
            | "usb_ac"
            | "usb_c"
            | "usb_dcp"
            | "usb_cdp"
            | "usb_aca"
            | "usb_pd"
            | "wireless"
    )
}

fn read_bool(path: impl AsRef<Path>) -> Option<bool> {
    match read_trimmed(path)?.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "online" => Some(true),
        "0" | "false" | "no" | "offline" => Some(false),
        _ => None,
    }
}

fn read_trimmed(path: impl AsRef<Path>) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn reports_unknown_when_power_supply_root_is_missing() {
        let root = temp_path("missing");
        assert_eq!(
            read_power_state_from_sysfs(&root).unwrap(),
            PowerState::Unknown
        );
    }

    #[test]
    fn reports_battery_for_discharging_system_battery() {
        let root = TempDir::new("battery");
        write_supply(
            root.path(),
            "BAT0",
            &[
                ("type", "Battery"),
                ("scope", "System"),
                ("status", "Discharging"),
            ],
        );

        assert_eq!(
            read_power_state_from_sysfs(root.path()).unwrap(),
            PowerState::Battery
        );
    }

    #[test]
    fn reports_ac_when_external_supply_is_online() {
        let root = TempDir::new("ac");
        write_supply(
            root.path(),
            "BAT0",
            &[
                ("type", "Battery"),
                ("scope", "System"),
                ("status", "Discharging"),
            ],
        );
        write_supply(root.path(), "AC", &[("type", "Mains"), ("online", "1")]);

        assert_eq!(
            read_power_state_from_sysfs(root.path()).unwrap(),
            PowerState::Ac
        );
    }

    #[test]
    fn ignores_device_scope_batteries() {
        let root = TempDir::new("device-battery");
        write_supply(
            root.path(),
            "mouse",
            &[
                ("type", "Battery"),
                ("scope", "Device"),
                ("status", "Discharging"),
            ],
        );

        assert_eq!(
            read_power_state_from_sysfs(root.path()).unwrap(),
            PowerState::Unknown
        );
    }

    fn write_supply(root: &Path, name: &str, fields: &[(&str, &str)]) {
        let path = root.join(name);
        fs::create_dir_all(&path).unwrap();
        for (field, value) in fields {
            fs::write(path.join(field), value).unwrap();
        }
    }

    fn temp_path(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("gilder-power-test-{nonce}-{name}"))
    }

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(name: &str) -> Self {
            let path = temp_path(name);
            fs::create_dir_all(&path).unwrap();
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}
