#[allow(dead_code)]
mod codec;
#[allow(dead_code)]
mod message;
mod server;

pub use server::{IpcError, IpcServer};
