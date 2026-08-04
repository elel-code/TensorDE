use vulkan_renderer::{AdapterSelector, PciAddress, PowerPreference};

const DEVICE_ENV: &str = "TENSOR_WALLPAPER_RENDER_DEVICE";
const PREFERENCE_ENV: &str = "TENSOR_WALLPAPER_RENDER_DEVICE_PREFERENCE";

pub(super) struct SharedAdapterPolicy {
    pub(super) preference: PowerPreference,
    pub(super) selector: Option<AdapterSelector>,
}

impl SharedAdapterPolicy {
    pub(super) fn from_environment() -> Result<Self, String> {
        let selector = environment_value(DEVICE_ENV)?
            .filter(|value| !value.trim().is_empty())
            .map(|value| parse_selector(&value))
            .transpose()?;
        let preference = environment_value(PREFERENCE_ENV)?
            .filter(|value| !value.trim().is_empty())
            .map(|value| parse_preference(&value))
            .transpose()?
            .unwrap_or(PowerPreference::Discrete);
        Ok(Self {
            preference,
            selector,
        })
    }
}

fn environment_value(name: &str) -> Result<Option<String>, String> {
    std::env::var_os(name)
        .map(|value| {
            value
                .into_string()
                .map_err(|_| format!("{name} is not valid UTF-8"))
        })
        .transpose()
}

fn parse_selector(value: &str) -> Result<AdapterSelector, String> {
    let value = value.trim();
    if let Some(index) = value.strip_prefix("index:") {
        return index
            .parse::<usize>()
            .map(AdapterSelector::Ordinal)
            .map_err(|_| format!("{DEVICE_ENV} has invalid index selector {value:?}"));
    }
    if let Some(name) = value.strip_prefix("name:") {
        let name = name.trim();
        if name.is_empty() {
            return Err(format!("{DEVICE_ENV} name selector is empty"));
        }
        return Ok(AdapterSelector::NameContains(name.into()));
    }
    if let Some(uuid) = value.strip_prefix("uuid:") {
        return parse_uuid(uuid).map(AdapterSelector::DeviceUuid);
    }
    if let Some(pci) = value.strip_prefix("pci:") {
        return parse_pci(pci).map(AdapterSelector::Pci);
    }
    Ok(AdapterSelector::NameContains(value.into()))
}

fn parse_preference(value: &str) -> Result<PowerPreference, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "discrete" => Ok(PowerPreference::Discrete),
        "integrated" => Ok(PowerPreference::Integrated),
        "enumeration" => Ok(PowerPreference::Any),
        _ => Err(format!(
            "{PREFERENCE_ENV} must be discrete, integrated, or enumeration; got {value:?}"
        )),
    }
}

fn parse_uuid(value: &str) -> Result<[u8; 16], String> {
    let digits = value.replace('-', "");
    if digits.len() != 32 {
        return Err(format!(
            "{DEVICE_ENV} UUID requires 32 hexadecimal digits; got {}",
            digits.len()
        ));
    }
    let mut uuid = [0; 16];
    for (index, byte) in uuid.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&digits[index * 2..index * 2 + 2], 16)
            .map_err(|_| format!("{DEVICE_ENV} UUID {value:?} contains a non-hexadecimal digit"))?;
    }
    Ok(uuid)
}

fn parse_pci(value: &str) -> Result<PciAddress, String> {
    let (bus_path, function) = value
        .rsplit_once('.')
        .ok_or_else(|| format!("{DEVICE_ENV} PCI selector must be domain:bus:device.function"))?;
    let components = bus_path.split(':').collect::<Vec<_>>();
    let (domain, bus, device) = match components.as_slice() {
        [bus, device] => (0, parse_hex(bus)?, parse_hex(device)?),
        [domain, bus, device] => (parse_hex(domain)?, parse_hex(bus)?, parse_hex(device)?),
        _ => {
            return Err(format!(
                "{DEVICE_ENV} PCI selector must be domain:bus:device.function"
            ));
        }
    };
    Ok(PciAddress {
        domain,
        bus,
        device,
        function: parse_hex(function)?,
    })
}

fn parse_hex(value: &str) -> Result<u32, String> {
    u32::from_str_radix(value, 16)
        .map_err(|_| format!("{DEVICE_ENV} has invalid hexadecimal component {value:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selectors_preserve_existing_product_syntax() {
        assert_eq!(parse_selector("index:2"), Ok(AdapterSelector::Ordinal(2)));
        assert_eq!(
            parse_selector("name:RADV"),
            Ok(AdapterSelector::NameContains("RADV".into()))
        );
        assert_eq!(
            parse_selector("pci:0000:0a:00.1"),
            Ok(AdapterSelector::Pci(PciAddress {
                domain: 0,
                bus: 10,
                device: 0,
                function: 1,
            }))
        );
        assert_eq!(
            parse_selector("uuid:00010203-0405-0607-0809-0a0b0c0d0e0f"),
            Ok(AdapterSelector::DeviceUuid([
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15,
            ]))
        );
    }
}
