#![deny(unsafe_code)]

mod compositor;
mod config;
mod ecs;
mod ipc;
mod layout;
mod protocol;
mod render;
mod startup;

#[cfg(feature = "systemd")]
mod service;

fn main() -> Result<(), startup::StartupError> {
    startup::run()
}
