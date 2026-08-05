use std::env;
use std::path::PathBuf;

pub const SOCKET_DIR_NAME: &str = "tensor-wallpaper";
pub const SOCKET_FILE_NAME: &str = "tensor-wallpaper.sock";

pub fn runtime_socket_path() -> Option<PathBuf> {
    runtime_socket_path_from_env(
        env::var_os("TENSOR_WALLPAPER_SOCKET"),
        env::var_os("XDG_RUNTIME_DIR"),
    )
}

fn runtime_socket_path_from_env(
    tensor_wallpaper_socket: Option<impl Into<PathBuf>>,
    xdg_runtime_dir: Option<impl Into<PathBuf>>,
) -> Option<PathBuf> {
    if let Some(socket) = tensor_wallpaper_socket {
        return Some(socket.into());
    }
    let runtime_dir = xdg_runtime_dir?;
    Some(
        runtime_dir
            .into()
            .join(SOCKET_DIR_NAME)
            .join(SOCKET_FILE_NAME),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_explicit_tensor_wallpaper_socket() {
        let socket = runtime_socket_path_from_env(Some("/tmp/custom.sock"), Some("/tmp/runtime"));
        assert_eq!(socket, Some(PathBuf::from("/tmp/custom.sock")));
    }

    #[test]
    fn resolves_default_socket_under_xdg_runtime_dir() {
        let socket = runtime_socket_path_from_env(None::<&str>, Some("/tmp/runtime"));
        assert_eq!(
            socket,
            Some(PathBuf::from(
                "/tmp/runtime/tensor-wallpaper/tensor-wallpaper.sock"
            ))
        );
    }
}
