#![deny(unsafe_code)]

mod backend;
mod compositor;
mod config;
pub mod ecs;
pub mod ipc;
pub mod layout;
mod protocol;
mod render;
pub mod scene;
pub mod service;
mod signals;
pub mod spawn;
pub mod startup;
mod xwayland;
