//! Connector / CRTC identity shared across policy, event bus, and present.

use tensor_event::OutputId;

/// Stable connector identity: DRM device + connector object id.
///
/// Packs into [`OutputId`] for the event bus without allocating.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ConnectorId {
    pub device_id: u64,
    pub connector_id: u32,
}

impl ConnectorId {
    #[inline]
    pub const fn new(device_id: u64, connector_id: u32) -> Self {
        Self {
            device_id,
            connector_id,
        }
    }

    /// Pack into a bus [`OutputId`].
    ///
    /// Layout: high 32 bits = low 32 of `device_id`, low 32 = `connector_id`.
    /// Device ids that only differ above 32 bits collide — acceptable for a
    /// single-session compositor with few DRM cards; adapters must keep the
    /// full map if they need the high bits.
    #[inline]
    pub const fn as_output_id(self) -> OutputId {
        let packed = ((self.device_id as u32 as u64) << 32) | (self.connector_id as u64);
        OutputId::new(packed)
    }

    #[inline]
    pub const fn from_output_id(id: OutputId) -> Self {
        let raw = id.get();
        Self {
            device_id: raw >> 32,
            connector_id: raw as u32,
        }
    }
}

/// Hotplug / link state of a connector (value-only).
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum ConnectorState {
    Connected,
    Disconnected,
    #[default]
    Unknown,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connector_id_roundtrips_through_output_id() {
        let id = ConnectorId::new(0xABu64, 42);
        assert_eq!(ConnectorId::from_output_id(id.as_output_id()), id);
    }

    #[test]
    fn packing_uses_low_device_bits() {
        let id = ConnectorId::new(0x1_0000_0007, 9);
        let unpacked = ConnectorId::from_output_id(id.as_output_id());
        assert_eq!(unpacked.device_id, 7);
        assert_eq!(unpacked.connector_id, 9);
    }
}
