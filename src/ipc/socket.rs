use std::env;
use std::path::PathBuf;

pub const SOCKET_DIR_NAME: &str = "gilder";
pub const SOCKET_FILE_NAME: &str = "gilder.sock";

pub fn runtime_socket_path() -> Option<PathBuf> {
    let runtime_dir = env::var_os("XDG_RUNTIME_DIR")?;
    Some(
        PathBuf::from(runtime_dir)
            .join(SOCKET_DIR_NAME)
            .join(SOCKET_FILE_NAME),
    )
}
