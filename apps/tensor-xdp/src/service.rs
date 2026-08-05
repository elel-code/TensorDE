use tensor_dbus::{
    Connection, MethodError, ObjectServer, PropertyChangeMode, RequestNameFlags, RequestNameReply,
};
use thiserror::Error;

use crate::{
    SettingsError, SettingsSnapshot,
    settings::{SETTINGS_INTERFACE, SETTINGS_VERSION, SettingsMap},
};

pub const BUS_NAME: &str = "org.freedesktop.impl.portal.desktop.tensor";
pub const OBJECT_PATH: &str = "/org/freedesktop/portal/desktop";
#[cfg(test)]
const PROPERTIES_INTERFACE: &str = "org.freedesktop.DBus.Properties";
#[cfg(test)]
const INTROSPECTABLE_INTERFACE: &str = "org.freedesktop.DBus.Introspectable";

#[derive(Clone, Copy, Debug)]
pub struct SettingsService {
    settings: SettingsSnapshot,
}

impl SettingsService {
    pub const fn new(settings: SettingsSnapshot) -> Self {
        Self { settings }
    }

    fn object_server(self) -> Result<ObjectServer, tensor_dbus::Error> {
        let mut objects = ObjectServer::new();
        let settings = self.settings;
        objects.register::<Vec<String>, SettingsMap, _, _>(
            OBJECT_PATH,
            SETTINGS_INTERFACE,
            "ReadAll",
            move |namespaces| {
                let result = settings.read_all(&namespaces).map_err(map_read_all_error);
                async move { result }
            },
        )?;
        let settings = self.settings;
        objects.register::<(String, String), tensor_dbus::zvariant::OwnedValue, _, _>(
            OBJECT_PATH,
            SETTINGS_INTERFACE,
            "Read",
            move |(namespace, key)| {
                let result = settings.read(&namespace, &key).map_err(map_read_error);
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
        loop {
            drop(objects.serve_next(&mut connection).await?);
        }
    }
}

fn map_read_all_error(error: SettingsError) -> MethodError {
    match error {
        SettingsError::InvalidFilters => MethodError::invalid_args(error),
        SettingsError::NotFound => unreachable!("ReadAll never reports NotFound"),
    }
}

fn map_read_error(error: SettingsError) -> MethodError {
    match error {
        SettingsError::NotFound => {
            MethodError::new("org.freedesktop.portal.Error.NotFound", error.to_string())
        }
        SettingsError::InvalidFilters => unreachable!("Read has no filters"),
    }
}

#[derive(Debug, Error)]
pub enum ServiceError {
    #[error("Tensor XDP D-Bus transport failed: {0}")]
    Dbus(#[from] tensor_dbus::Error),
    #[error("Tensor XDP bus name is already owned: {0:?}")]
    NameUnavailable(RequestNameReply),
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
        std::env::current_dir()
            .unwrap()
            .join("target")
            .join(format!(
                "tensor-xdp-test-{}-{}.sock",
                std::process::id(),
                NEXT_BUS.fetch_add(1, Ordering::Relaxed)
            ))
    }

    #[test]
    fn settings_service_uses_the_typed_object_server_over_p2p() {
        let path = socket_path();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
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
}
