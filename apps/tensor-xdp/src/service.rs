use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

use futures_util::{FutureExt, pin_mut, select_biased};
use tensor_dbus::{
    Connection, MethodError, ObjectServer, PropertyChangeMode, RequestNameFlags, RequestNameReply,
};
use thiserror::Error;

use crate::{
    SettingsError, SettingsSnapshot, TensorXdpConfig,
    settings::{SETTINGS_INTERFACE, SETTINGS_VERSION, SettingsMap},
};

pub const BUS_NAME: &str = "org.freedesktop.impl.portal.desktop.tensor";
pub const OBJECT_PATH: &str = "/org/freedesktop/portal/desktop";
#[cfg(test)]
const PROPERTIES_INTERFACE: &str = "org.freedesktop.DBus.Properties";
#[cfg(test)]
const INTROSPECTABLE_INTERFACE: &str = "org.freedesktop.DBus.Introspectable";

const RELOAD_INTERVAL: Duration = Duration::from_secs(1);

type SharedSettings = Arc<Mutex<SettingsSnapshot>>;

#[derive(Clone, Debug)]
pub struct SettingsService {
    settings: SharedSettings,
}

impl SettingsService {
    pub fn new(settings: SettingsSnapshot) -> Self {
        Self {
            settings: Arc::new(Mutex::new(settings)),
        }
    }

    fn object_server(&self) -> Result<ObjectServer, tensor_dbus::Error> {
        let mut objects = ObjectServer::new();
        let settings = Arc::clone(&self.settings);
        objects.register::<Vec<String>, SettingsMap, _, _>(
            OBJECT_PATH,
            SETTINGS_INTERFACE,
            "ReadAll",
            move |namespaces| {
                let result = read_all(&settings, &namespaces).map_err(map_read_all_error);
                async move { result }
            },
        )?;
        let settings = Arc::clone(&self.settings);
        objects.register::<(String, String), tensor_dbus::zvariant::OwnedValue, _, _>(
            OBJECT_PATH,
            SETTINGS_INTERFACE,
            "Read",
            move |(namespace, key)| {
                let result = read(&settings, &namespace, &key).map_err(map_read_error);
                async move { result }
            },
        )?;
        objects.register_signal::<(String, String, tensor_dbus::zvariant::OwnedValue)>(
            OBJECT_PATH,
            SETTINGS_INTERFACE,
            "SettingChanged",
        )?;
        objects.register_read_only_property::<u32, _, _>(
            OBJECT_PATH,
            SETTINGS_INTERFACE,
            "version",
            PropertyChangeMode::Const,
            || async { Ok::<_, MethodError>(SETTINGS_VERSION) },
        )?;
        Ok(objects)
    }

    pub async fn run(self) -> Result<(), ServiceError> {
        self.run_inner(None).await
    }

    pub async fn run_with_reload(
        self,
        config_path: impl Into<PathBuf>,
    ) -> Result<(), ServiceError> {
        self.run_inner(Some(config_path.into())).await
    }

    async fn run_inner(self, config_path: Option<PathBuf>) -> Result<(), ServiceError> {
        let mut connection = Connection::session_bus().await?;
        let name = connection
            .request_name(BUS_NAME, RequestNameFlags::DO_NOT_QUEUE)
            .await?;
        if !matches!(
            name,
            RequestNameReply::PrimaryOwner | RequestNameReply::AlreadyOwner
        ) {
            return Err(ServiceError::NameUnavailable(name));
        }

        let mut objects = self.object_server()?;
        let Some(config_path) = config_path else {
            loop {
                drop(objects.serve_next(&mut connection).await?);
            }
        };
        loop {
            let reload = {
                let serve = objects.serve_next(&mut connection).fuse();
                let timer = compio::runtime::time::sleep(RELOAD_INTERVAL).fuse();
                pin_mut!(serve, timer);
                select_biased! {
                    result = serve => {
                        drop(result?);
                        false
                    }
                    _ = timer => true,
                }
            };
            if reload {
                self.reload_if_changed(&config_path, &mut connection)
                    .await?;
            }
        }
    }

    async fn reload_if_changed(
        &self,
        path: &Path,
        connection: &mut Connection,
    ) -> Result<(), ServiceError> {
        let config = match TensorXdpConfig::load_or_default(path) {
            Ok(config) => config,
            Err(error) => {
                eprintln!(
                    "tensor-xdp: retaining last valid settings after reload failure: {error}"
                );
                return Ok(());
            }
        };
        let next = SettingsSnapshot::new(config.appearance);
        let changes = self.replace_snapshot(next)?;
        for (namespace, key, value) in changes {
            connection
                .emit_signal(
                    OBJECT_PATH,
                    SETTINGS_INTERFACE,
                    "SettingChanged",
                    &(namespace, key, value),
                )
                .await?;
        }
        Ok(())
    }

    fn replace_snapshot(
        &self,
        next: SettingsSnapshot,
    ) -> Result<Vec<(String, String, tensor_dbus::zvariant::OwnedValue)>, ServiceError> {
        let mut settings = self
            .settings
            .lock()
            .map_err(|_| ServiceError::StatePoisoned)?;
        let changes = settings.changed_values(&next);
        *settings = next;
        Ok(changes)
    }
}

fn read_all(
    settings: &SharedSettings,
    namespaces: &[String],
) -> Result<SettingsMap, SettingsError> {
    settings
        .lock()
        .map_err(|_| SettingsError::Unavailable)?
        .read_all(namespaces)
}

fn read(
    settings: &SharedSettings,
    namespace: &str,
    key: &str,
) -> Result<tensor_dbus::zvariant::OwnedValue, SettingsError> {
    settings
        .lock()
        .map_err(|_| SettingsError::Unavailable)?
        .read(namespace, key)
}

fn map_read_all_error(error: SettingsError) -> MethodError {
    match error {
        SettingsError::InvalidFilters => MethodError::invalid_args(error),
        SettingsError::NotFound => unreachable!("ReadAll never reports NotFound"),
        SettingsError::Unavailable => MethodError::failed(error.to_string()),
    }
}

fn map_read_error(error: SettingsError) -> MethodError {
    match error {
        SettingsError::NotFound => {
            MethodError::new("org.freedesktop.portal.Error.NotFound", error.to_string())
        }
        SettingsError::InvalidFilters => unreachable!("Read has no filters"),
        SettingsError::Unavailable => MethodError::failed(error.to_string()),
    }
}

#[derive(Debug, Error)]
pub enum ServiceError {
    #[error("Tensor XDP D-Bus transport failed: {0}")]
    Dbus(#[from] tensor_dbus::Error),
    #[error("Tensor XDP bus name is already owned: {0:?}")]
    NameUnavailable(RequestNameReply),
    #[error("Tensor XDP settings snapshot lock is poisoned")]
    StatePoisoned,
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };

    use compio::net::UnixStream;
    use tensor_dbus::{Error, Guid, PeerListener, Proxy};
    use tensor_runtime::io_uring_runtime;

    use super::*;
    use crate::AppearanceSettings;

    fn socket_path() -> PathBuf {
        static NEXT_BUS: AtomicU64 = AtomicU64::new(1);
        std::env::temp_dir().join(format!(
            "tensor-xdp-test-{}-{}.sock",
            std::process::id(),
            NEXT_BUS.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn settings_service_uses_the_typed_object_server_over_p2p() {
        let path = socket_path();
        let _ = fs::remove_file(&path);
        let guid = Guid::generate().unwrap();
        let runtime = io_uring_runtime(4).expect("io_uring runtime is required");

        runtime.block_on(async {
            let listener = PeerListener::bind(&path, guid).await.unwrap();
            let service =
                SettingsService::new(SettingsSnapshot::new(AppearanceSettings::default()));
            let server = compio::runtime::spawn(async move {
                let mut server_connection = listener.accept_authenticated().await.unwrap();
                let mut served = 0;
                let mut objects = service.object_server().unwrap();
                while served < 7 {
                    if objects
                        .serve_next(&mut server_connection)
                        .await
                        .unwrap()
                        .is_none()
                    {
                        served += 1;
                    }
                }
            });

            let stream = UnixStream::connect(&path).await.unwrap();
            let mut client = Connection::connect_peer(stream, Some(guid)).await.unwrap();
            let mut settings = Proxy::new(
                &mut client,
                None::<&str>,
                OBJECT_PATH,
                Some(SETTINGS_INTERFACE),
            )
            .unwrap();
            let value: tensor_dbus::zvariant::OwnedValue = settings
                .call("Read", &("org.freedesktop.appearance", "color-scheme"))
                .await
                .unwrap();
            assert_eq!(u32::try_from(value).unwrap(), 0);

            let not_found = settings
                .call::<_, tensor_dbus::zvariant::OwnedValue>(
                    "Read",
                    &("org.freedesktop.appearance", "missing"),
                )
                .await
                .unwrap_err();
            assert!(matches!(
                not_found,
                Error::Method { ref name, ref message }
                    if name == "org.freedesktop.portal.Error.NotFound"
                        && message == "unknown settings namespace or key"
            ));

            let invalid_args = settings
                .call::<_, tensor_dbus::zvariant::OwnedValue>("Read", &42_u32)
                .await
                .unwrap_err();
            assert!(matches!(
                invalid_args,
                Error::Method { ref name, .. }
                    if name == "org.freedesktop.DBus.Error.InvalidArgs"
            ));
            drop(settings);

            let mut properties = Proxy::new(
                &mut client,
                None::<&str>,
                OBJECT_PATH,
                Some(PROPERTIES_INTERFACE),
            )
            .unwrap();
            let version: tensor_dbus::zvariant::OwnedValue = properties
                .call("Get", &(SETTINGS_INTERFACE, "version"))
                .await
                .unwrap();
            assert_eq!(u32::try_from(version).unwrap(), SETTINGS_VERSION);

            let unknown_property = properties
                .call::<_, tensor_dbus::zvariant::OwnedValue>(
                    "Get",
                    &(SETTINGS_INTERFACE, "missing"),
                )
                .await
                .unwrap_err();
            assert!(matches!(
                unknown_property,
                Error::Method { ref name, .. }
                    if name == "org.freedesktop.DBus.Error.UnknownProperty"
            ));
            let all: std::collections::HashMap<String, tensor_dbus::zvariant::OwnedValue> =
                properties
                    .call("GetAll", &(SETTINGS_INTERFACE,))
                    .await
                    .unwrap();
            assert_eq!(
                u32::try_from(all.get("version").unwrap()).unwrap(),
                SETTINGS_VERSION
            );
            drop(properties);

            let mut introspectable = Proxy::new(
                &mut client,
                None::<&str>,
                OBJECT_PATH,
                Some(INTROSPECTABLE_INTERFACE),
            )
            .unwrap();
            let xml: String = introspectable.call("Introspect", &()).await.unwrap();
            assert!(xml.contains(SETTINGS_INTERFACE));
            assert!(xml.contains("ReadAll"));
            assert!(xml.contains("SettingChanged"));
            assert!(xml.contains("EmitsChangedSignal\" value=\"const"));
            assert!(!xml.contains("FileChooser"));
            assert!(!xml.contains("ScreenCast"));
            server.await.unwrap();
        });
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn live_snapshot_replacement_is_atomic_and_noop_when_values_are_unchanged() {
        let service = SettingsService::new(SettingsSnapshot::new(AppearanceSettings::default()));
        let next = SettingsSnapshot::new(AppearanceSettings {
            color_scheme: crate::ColorScheme::Dark,
            contrast: crate::Contrast::Normal,
            reduced_motion: true,
        });
        let changes = service.replace_snapshot(next).unwrap();
        assert_eq!(changes.len(), 2);
        assert!(service.replace_snapshot(next).unwrap().is_empty());

        let objects = service.object_server().unwrap();
        assert!(objects.contains_object(OBJECT_PATH));
    }
}
