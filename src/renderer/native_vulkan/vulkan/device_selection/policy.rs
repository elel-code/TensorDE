//! Pure parsing, matching, and ranking policy for Vulkan physical devices.

use vulkanalia::vk;

const DEVICE_ENV: &str = "GILDER_VULKAN_DEVICE";
const PREFERENCE_ENV: &str = "GILDER_VULKAN_DEVICE_PREFERENCE";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct NativeVulkanPciAddress {
    pub domain: u32,
    pub bus: u32,
    pub device: u32,
    pub function: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum NativeVulkanDeviceSelector {
    Index(usize),
    Name(String),
    Uuid([u8; vk::UUID_SIZE]),
    Pci(NativeVulkanPciAddress),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeVulkanDevicePreference {
    Discrete,
    Integrated,
    Enumeration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct NativeVulkanDeviceSelectionPolicy {
    selector: Option<NativeVulkanDeviceSelector>,
    preference: NativeVulkanDevicePreference,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct NativeVulkanDeviceCandidate {
    pub original_index: usize,
    pub name: String,
    pub device_type: vk::PhysicalDeviceType,
    pub device_uuid: [u8; vk::UUID_SIZE],
    pub pci_address: Option<NativeVulkanPciAddress>,
}

impl NativeVulkanDeviceSelectionPolicy {
    pub(super) fn from_environment() -> Result<Self, String> {
        let selector = environment_value(DEVICE_ENV)?
            .filter(|value| !value.trim().is_empty())
            .map(|value| parse_selector(&value))
            .transpose()?;
        let preference = environment_value(PREFERENCE_ENV)?
            .filter(|value| !value.trim().is_empty())
            .map(|value| parse_preference(&value))
            .transpose()?
            .unwrap_or(NativeVulkanDevicePreference::Discrete);
        Ok(Self {
            selector,
            preference,
        })
    }
}

pub(super) fn ordered_candidate_positions(
    policy: &NativeVulkanDeviceSelectionPolicy,
    candidates: &[NativeVulkanDeviceCandidate],
) -> Result<Vec<usize>, String> {
    if let Some(selector) = &policy.selector {
        let matches = candidates
            .iter()
            .enumerate()
            .filter_map(|(position, candidate)| {
                selector_matches(selector, candidate).then_some(position)
            })
            .collect::<Vec<_>>();
        return match matches.as_slice() {
            [position] => Ok(vec![*position]),
            [] => Err(format!(
                "{DEVICE_ENV}={:?} matches no Vulkan physical device; available: {}",
                selector_label(selector),
                available_device_labels(candidates)
            )),
            _ => Err(format!(
                "{DEVICE_ENV}={:?} is ambiguous; matches: {}",
                selector_label(selector),
                matches
                    .iter()
                    .map(|position| candidate_label(&candidates[*position]))
                    .collect::<Vec<_>>()
                    .join("; ")
            )),
        };
    }

    let mut positions = (0..candidates.len()).collect::<Vec<_>>();
    positions.sort_by_key(|position| {
        let candidate = &candidates[*position];
        (
            device_type_rank(policy.preference, candidate.device_type),
            candidate.original_index,
        )
    });
    Ok(positions)
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

fn parse_selector(value: &str) -> Result<NativeVulkanDeviceSelector, String> {
    let value = value.trim();
    if let Some(index) = value.strip_prefix("index:") {
        return index
            .parse::<usize>()
            .map(NativeVulkanDeviceSelector::Index)
            .map_err(|_| format!("{DEVICE_ENV} has invalid index selector {value:?}"));
    }
    if let Some(name) = value.strip_prefix("name:") {
        let name = name.trim();
        if name.is_empty() {
            return Err(format!("{DEVICE_ENV} name selector is empty"));
        }
        return Ok(NativeVulkanDeviceSelector::Name(name.to_owned()));
    }
    if let Some(uuid) = value.strip_prefix("uuid:") {
        return parse_uuid(uuid)
            .map(NativeVulkanDeviceSelector::Uuid)
            .map_err(|err| format!("{DEVICE_ENV} has invalid UUID selector: {err}"));
    }
    if let Some(pci) = value.strip_prefix("pci:") {
        return parse_pci_address(pci)
            .map(NativeVulkanDeviceSelector::Pci)
            .map_err(|err| format!("{DEVICE_ENV} has invalid PCI selector: {err}"));
    }
    Ok(NativeVulkanDeviceSelector::Name(value.to_owned()))
}

fn parse_preference(value: &str) -> Result<NativeVulkanDevicePreference, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "discrete" | "auto" => Ok(NativeVulkanDevicePreference::Discrete),
        "integrated" => Ok(NativeVulkanDevicePreference::Integrated),
        "enumeration" => Ok(NativeVulkanDevicePreference::Enumeration),
        _ => Err(format!(
            "{PREFERENCE_ENV} must be discrete, integrated, or enumeration; got {value:?}"
        )),
    }
}

fn parse_uuid(value: &str) -> Result<[u8; vk::UUID_SIZE], String> {
    let digits = value
        .chars()
        .filter(|character| *character != '-')
        .collect::<String>();
    if digits.len() != vk::UUID_SIZE * 2 {
        return Err(format!(
            "expected {} hexadecimal digits, got {}",
            vk::UUID_SIZE * 2,
            digits.len()
        ));
    }
    let mut uuid = [0u8; vk::UUID_SIZE];
    for (index, byte) in uuid.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&digits[index * 2..index * 2 + 2], 16)
            .map_err(|_| format!("{value:?} contains a non-hexadecimal digit"))?;
    }
    Ok(uuid)
}

fn parse_pci_address(value: &str) -> Result<NativeVulkanPciAddress, String> {
    let (bus_path, function) = value
        .rsplit_once('.')
        .ok_or_else(|| format!("expected domain:bus:device.function, got {value:?}"))?;
    let components = bus_path.split(':').collect::<Vec<_>>();
    let (domain, bus, device) = match components.as_slice() {
        [bus, device] => (0, parse_hex(bus)?, parse_hex(device)?),
        [domain, bus, device] => (parse_hex(domain)?, parse_hex(bus)?, parse_hex(device)?),
        _ => return Err(format!("expected domain:bus:device.function, got {value:?}")),
    };
    Ok(NativeVulkanPciAddress {
        domain,
        bus,
        device,
        function: parse_hex(function)?,
    })
}

fn parse_hex(value: &str) -> Result<u32, String> {
    u32::from_str_radix(value, 16).map_err(|_| format!("invalid hexadecimal component {value:?}"))
}

fn selector_matches(
    selector: &NativeVulkanDeviceSelector,
    candidate: &NativeVulkanDeviceCandidate,
) -> bool {
    match selector {
        NativeVulkanDeviceSelector::Index(index) => candidate.original_index == *index,
        NativeVulkanDeviceSelector::Name(name) => candidate
            .name
            .to_ascii_lowercase()
            .contains(&name.to_ascii_lowercase()),
        NativeVulkanDeviceSelector::Uuid(uuid) => candidate.device_uuid == *uuid,
        NativeVulkanDeviceSelector::Pci(address) => candidate.pci_address == Some(*address),
    }
}

fn device_type_rank(
    preference: NativeVulkanDevicePreference,
    device_type: vk::PhysicalDeviceType,
) -> u8 {
    match preference {
        NativeVulkanDevicePreference::Discrete => match device_type {
            vk::PhysicalDeviceType::DISCRETE_GPU => 0,
            vk::PhysicalDeviceType::INTEGRATED_GPU => 1,
            vk::PhysicalDeviceType::VIRTUAL_GPU => 2,
            vk::PhysicalDeviceType::OTHER => 3,
            vk::PhysicalDeviceType::CPU => 4,
            _ => 5,
        },
        NativeVulkanDevicePreference::Integrated => match device_type {
            vk::PhysicalDeviceType::INTEGRATED_GPU => 0,
            vk::PhysicalDeviceType::DISCRETE_GPU => 1,
            vk::PhysicalDeviceType::VIRTUAL_GPU => 2,
            vk::PhysicalDeviceType::OTHER => 3,
            vk::PhysicalDeviceType::CPU => 4,
            _ => 5,
        },
        NativeVulkanDevicePreference::Enumeration => 0,
    }
}

fn selector_label(selector: &NativeVulkanDeviceSelector) -> String {
    match selector {
        NativeVulkanDeviceSelector::Index(index) => format!("index:{index}"),
        NativeVulkanDeviceSelector::Name(name) => format!("name:{name}"),
        NativeVulkanDeviceSelector::Uuid(uuid) => format!("uuid:{}", uuid_label(*uuid)),
        NativeVulkanDeviceSelector::Pci(address) => format!("pci:{}", pci_label(*address)),
    }
}

fn available_device_labels(candidates: &[NativeVulkanDeviceCandidate]) -> String {
    if candidates.is_empty() {
        return "none".to_owned();
    }
    candidates
        .iter()
        .map(candidate_label)
        .collect::<Vec<_>>()
        .join("; ")
}

fn candidate_label(candidate: &NativeVulkanDeviceCandidate) -> String {
    let pci = candidate
        .pci_address
        .map(pci_label)
        .unwrap_or_else(|| "unavailable".to_owned());
    format!(
        "index:{} name={:?} type={:?} uuid={} pci={}",
        candidate.original_index,
        candidate.name,
        candidate.device_type,
        uuid_label(candidate.device_uuid),
        pci,
    )
}

fn uuid_label(uuid: [u8; vk::UUID_SIZE]) -> String {
    uuid.iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>()
}

fn pci_label(address: NativeVulkanPciAddress) -> String {
    format!(
        "{:04x}:{:02x}:{:02x}.{:x}",
        address.domain, address.bus, address.device, address.function
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_policy_prefers_discrete_over_enumeration_order() {
        let policy = NativeVulkanDeviceSelectionPolicy {
            selector: None,
            preference: NativeVulkanDevicePreference::Discrete,
        };
        let candidates = vec![
            candidate(0, "Intel", vk::PhysicalDeviceType::INTEGRATED_GPU),
            candidate(1, "NVIDIA", vk::PhysicalDeviceType::DISCRETE_GPU),
        ];

        assert_eq!(
            ordered_candidate_positions(&policy, &candidates).unwrap(),
            vec![1, 0]
        );
    }

    #[test]
    fn explicit_index_is_strict_and_overrides_preference() {
        let policy = NativeVulkanDeviceSelectionPolicy {
            selector: Some(NativeVulkanDeviceSelector::Index(0)),
            preference: NativeVulkanDevicePreference::Discrete,
        };
        let candidates = vec![
            candidate(0, "Intel", vk::PhysicalDeviceType::INTEGRATED_GPU),
            candidate(1, "NVIDIA", vk::PhysicalDeviceType::DISCRETE_GPU),
        ];

        assert_eq!(
            ordered_candidate_positions(&policy, &candidates).unwrap(),
            vec![0]
        );
    }

    #[test]
    fn ambiguous_name_selector_is_rejected() {
        let policy = NativeVulkanDeviceSelectionPolicy {
            selector: Some(NativeVulkanDeviceSelector::Name("gpu".to_owned())),
            preference: NativeVulkanDevicePreference::Discrete,
        };
        let candidates = vec![
            candidate(0, "Integrated GPU", vk::PhysicalDeviceType::INTEGRATED_GPU),
            candidate(1, "Discrete GPU", vk::PhysicalDeviceType::DISCRETE_GPU),
        ];

        assert!(
            ordered_candidate_positions(&policy, &candidates)
                .unwrap_err()
                .contains("ambiguous")
        );
    }

    #[test]
    fn uuid_and_pci_selectors_parse_canonical_forms() {
        assert_eq!(
            parse_selector("uuid:00112233-4455-6677-8899-aabbccddeeff").unwrap(),
            NativeVulkanDeviceSelector::Uuid([
                0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb,
                0xcc, 0xdd, 0xee, 0xff,
            ])
        );
        assert_eq!(
            parse_selector("pci:0000:01:00.0").unwrap(),
            NativeVulkanDeviceSelector::Pci(NativeVulkanPciAddress {
                domain: 0,
                bus: 1,
                device: 0,
                function: 0,
            })
        );
    }

    fn candidate(
        original_index: usize,
        name: &str,
        device_type: vk::PhysicalDeviceType,
    ) -> NativeVulkanDeviceCandidate {
        NativeVulkanDeviceCandidate {
            original_index,
            name: name.to_owned(),
            device_type,
            device_uuid: [original_index as u8; vk::UUID_SIZE],
            pci_address: None,
        }
    }
}
