//! Per-output scanout readiness (triple-buffer slots).

use tensor_host::{ConnectorId, PresentSlot, PresentState};

/// How many present slots a CRTC may pipeline (fixed; matches native triple buffer).
pub const SLOT_COUNT: u8 = 3;

/// Readiness of one present slot.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SlotReadiness {
    pub state: PresentState,
    pub serial: u64,
}

/// Per-output present readiness table (value-only).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutputReadiness {
    pub output: ConnectorId,
    slots: [SlotReadiness; SLOT_COUNT as usize],
    pub faulted: bool,
}

impl OutputReadiness {
    pub fn new(output: ConnectorId) -> Self {
        Self {
            output,
            slots: [SlotReadiness::default(); SLOT_COUNT as usize],
            faulted: false,
        }
    }

    #[inline]
    pub fn slot(&self, slot: PresentSlot) -> Option<&SlotReadiness> {
        self.slots.get(slot.0 as usize)
    }

    #[inline]
    pub fn slot_mut(&mut self, slot: PresentSlot) -> Option<&mut SlotReadiness> {
        self.slots.get_mut(slot.0 as usize)
    }

    /// Whether the slot can accept a new present (idle or already presented).
    pub fn ready_for(&self, slot: PresentSlot) -> bool {
        if self.faulted {
            return false;
        }
        matches!(
            self.slot(slot).map(|s| s.state),
            Some(PresentState::Idle | PresentState::Presented)
        )
    }

    pub fn mark_queued(&mut self, slot: PresentSlot, serial: u64) -> bool {
        if !self.ready_for(slot) {
            return false;
        }
        if let Some(s) = self.slot_mut(slot) {
            s.state = PresentState::Queued;
            s.serial = serial;
            true
        } else {
            false
        }
    }

    pub fn mark_waiting_vblank(&mut self, slot: PresentSlot) {
        if let Some(s) = self.slot_mut(slot)
            && s.state == PresentState::Queued
        {
            s.state = PresentState::WaitingForVBlank;
        }
    }

    pub fn mark_presented(&mut self, slot: PresentSlot) {
        if let Some(s) = self.slot_mut(slot) {
            s.state = PresentState::Presented;
        }
    }

    pub fn mark_faulted(&mut self) {
        self.faulted = true;
        for s in &mut self.slots {
            s.state = PresentState::Faulted;
        }
    }

    pub fn clear_fault(&mut self) {
        self.faulted = false;
        for s in &mut self.slots {
            *s = SlotReadiness::default();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn triple_buffer_queues_then_presents() {
        let mut ready = OutputReadiness::new(ConnectorId::new(1, 2));
        let slot = PresentSlot(0);
        assert!(ready.ready_for(slot));
        assert!(ready.mark_queued(slot, 10));
        assert!(!ready.ready_for(slot));
        ready.mark_waiting_vblank(slot);
        ready.mark_presented(slot);
        assert!(ready.ready_for(slot));
    }

    #[test]
    fn fault_blocks_all_slots() {
        let mut ready = OutputReadiness::new(ConnectorId::new(1, 2));
        ready.mark_faulted();
        assert!(!ready.ready_for(PresentSlot(0)));
        assert!(!ready.ready_for(PresentSlot(1)));
        ready.clear_fault();
        assert!(ready.ready_for(PresentSlot(0)));
    }
}
