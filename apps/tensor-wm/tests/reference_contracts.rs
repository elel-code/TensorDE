//! Behavioral contracts distilled from the local Niri, Hyprland, and Nourish
//! checkouts. These tests intentionally use Tensor's public boundaries rather
//! than importing an upstream fixture or implementation.

#[path = "reference_contracts/hyprland.rs"]
mod hyprland;
#[path = "reference_contracts/niri.rs"]
mod niri;
#[path = "reference_contracts/nourish.rs"]
mod nourish;
