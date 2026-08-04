//! Value-only DRM topology planning for Tensor.
//!
//! Policy that decides *which* connectors are active, at which mode/scale/
//! position, lives here. Opening DRM nodes, atomic commits, and GBM stay in
//! Tensor's native backend adapter. This crate has **zero** dependency on
//! libdrm or Wayland.

mod lease;
mod plan;
mod policy;
mod snapshot;

pub use lease::{
    ActiveLease, KernelLeaseId, LeaseConnector, LeaseError, LeaseOfferChanges, LeaseRegistry,
    LeaseReservation, LeaseRevocation, LeaseToken, MAX_ACTIVE_LEASES, MAX_CONNECTORS_PER_LEASE,
    MAX_LEASE_CONNECTORS,
};
pub use plan::{OutputPlan, OutputPlanDiff, PlanEvent, diff_plans, plan_outputs};
pub use policy::{
    OutputModeRequest, OutputRule, OutputRuleTable, guess_monitor_scale, highest_refresh_at_size,
    select_mode, select_requested_mode,
};
pub use snapshot::{ConnectorSnapshot, OutputDescriptor};
