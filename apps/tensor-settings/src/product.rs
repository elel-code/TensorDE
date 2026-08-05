use std::{env, path::PathBuf};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ProductKind {
    Land,
    Shell,
    Launcher,
    Wallpaper,
    Idle,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigFormat {
    Kdl,
    MigrationDebtToml,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReloadRoute {
    None,
    TensorMsgLand,
    TensorMsgWallpaper,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductEndpoint {
    pub product: ProductKind,
    pub config_path: PathBuf,
    pub config_format: ConfigFormat,
    pub socket_path: Option<PathBuf>,
    pub reload: ReloadRoute,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductRegistry {
    endpoints: Vec<ProductEndpoint>,
}

impl ProductRegistry {
    pub fn from_environment() -> Self {
        let config_home = xdg_config_home().unwrap_or_else(|| PathBuf::from("/etc"));
        let tensor_config = |name: &str| config_home.join("tensor").join(name);
        let runtime = env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from);
        let land_socket = env::var_os("TENSOR_IPC_SOCKET")
            .map(PathBuf::from)
            .or_else(|| runtime.as_ref().map(|path| path.join("tensor.sock")));
        let wallpaper_socket = env::var_os("TENSOR_WALLPAPER_SOCKET")
            .map(PathBuf::from)
            .or_else(|| {
                runtime
                    .as_ref()
                    .map(|path| path.join("tensor-wallpaper/tensor-wallpaper.sock"))
            });
        let land_config = env::var_os("TENSOR_CONFIG")
            .map(PathBuf::from)
            .unwrap_or_else(|| tensor_config("config.kdl"));

        Self {
            endpoints: vec![
                ProductEndpoint {
                    product: ProductKind::Land,
                    config_path: land_config,
                    config_format: ConfigFormat::Kdl,
                    socket_path: land_socket,
                    reload: ReloadRoute::TensorMsgLand,
                },
                ProductEndpoint {
                    product: ProductKind::Shell,
                    config_path: tensor_config("shell.kdl"),
                    config_format: ConfigFormat::Kdl,
                    socket_path: None,
                    reload: ReloadRoute::None,
                },
                ProductEndpoint {
                    product: ProductKind::Launcher,
                    config_path: tensor_config("launcher.kdl"),
                    config_format: ConfigFormat::Kdl,
                    socket_path: None,
                    reload: ReloadRoute::None,
                },
                ProductEndpoint {
                    product: ProductKind::Wallpaper,
                    config_path: config_home.join("tensor-wallpaper/config.toml"),
                    config_format: ConfigFormat::MigrationDebtToml,
                    socket_path: wallpaper_socket,
                    reload: ReloadRoute::TensorMsgWallpaper,
                },
                ProductEndpoint {
                    product: ProductKind::Idle,
                    config_path: tensor_config("idle.kdl"),
                    config_format: ConfigFormat::Kdl,
                    socket_path: None,
                    reload: ReloadRoute::None,
                },
            ],
        }
    }

    pub fn endpoints(&self) -> &[ProductEndpoint] {
        &self.endpoints
    }

    pub fn endpoint(&self, product: ProductKind) -> &ProductEndpoint {
        self.endpoints
            .iter()
            .find(|endpoint| endpoint.product == product)
            .expect("the fixed Tensor Settings registry contains every product")
    }
}

fn xdg_config_home() -> Option<PathBuf> {
    env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_keeps_product_policy_out_of_one_shared_daemon() {
        let registry = ProductRegistry::from_environment();
        assert_eq!(registry.endpoints().len(), 5);
        assert_eq!(
            registry.endpoint(ProductKind::Land).reload,
            ReloadRoute::TensorMsgLand
        );
        assert_eq!(
            registry.endpoint(ProductKind::Wallpaper).reload,
            ReloadRoute::TensorMsgWallpaper
        );
        assert_eq!(
            registry.endpoint(ProductKind::Idle).config_format,
            ConfigFormat::Kdl
        );
    }
}
