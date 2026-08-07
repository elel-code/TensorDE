//! Typed, caller-driven access to systemd-logind session locking.

use zvariant::{OwnedObjectPath, OwnedValue};

use crate::{Connection, Error, MatchRule, Message, Result as TransportResult};

pub const DESTINATION: &str = "org.freedesktop.login1";
pub const MANAGER_PATH: &str = "/org/freedesktop/login1";
pub const MANAGER_INTERFACE: &str = "org.freedesktop.login1.Manager";
pub const SESSION_INTERFACE: &str = "org.freedesktop.login1.Session";
pub const PROPERTIES_INTERFACE: &str = "org.freedesktop.DBus.Properties";

const SESSION_PATH_PREFIX: &str = "/org/freedesktop/login1/session/";

/// A validated logind session object discovered from a process or session id.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Login1Session {
    path: OwnedObjectPath,
}

impl Login1Session {
    pub fn from_path(path: OwnedObjectPath) -> Result<Self, Login1Error> {
        let valid = path
            .as_str()
            .strip_prefix(SESSION_PATH_PREFIX)
            .is_some_and(|object| !object.is_empty() && !object.contains('/'));
        if !valid {
            return Err(Login1Error::UnexpectedSessionPath {
                path: path.to_string(),
            });
        }
        Ok(Self { path })
    }

    pub async fn for_pid(connection: &mut Connection, pid: u32) -> Result<Self, Login1Error> {
        if pid == 0 {
            return Err(Login1Error::InvalidPid);
        }
        let path: OwnedObjectPath = connection
            .call(
                Some(DESTINATION),
                MANAGER_PATH,
                Some(MANAGER_INTERFACE),
                "GetSessionByPID",
                &(pid,),
            )
            .await?;
        Self::from_path(path)
    }

    pub async fn current(connection: &mut Connection) -> Result<Self, Login1Error> {
        Self::for_pid(connection, std::process::id()).await
    }

    pub async fn for_id(
        connection: &mut Connection,
        session_id: &str,
    ) -> Result<Self, Login1Error> {
        if session_id.is_empty() {
            return Err(Login1Error::InvalidSessionId);
        }
        let path: OwnedObjectPath = connection
            .call(
                Some(DESTINATION),
                MANAGER_PATH,
                Some(MANAGER_INTERFACE),
                "GetSession",
                &(session_id,),
            )
            .await?;
        Self::from_path(path)
    }

    pub fn path(&self) -> &OwnedObjectPath {
        &self.path
    }

    pub async fn locked_hint(&self, connection: &mut Connection) -> Result<bool, Login1Error> {
        let value: OwnedValue = connection
            .call(
                Some(DESTINATION),
                self.path.as_str(),
                Some(PROPERTIES_INTERFACE),
                "Get",
                &(SESSION_INTERFACE, "LockedHint"),
            )
            .await?;
        bool::try_from(&value).map_err(|source| Login1Error::InvalidProperty {
            property: "LockedHint",
            source,
        })
    }

    pub async fn set_locked_hint(
        &self,
        connection: &mut Connection,
        locked: bool,
    ) -> Result<(), Login1Error> {
        let (): () = connection
            .call(
                Some(DESTINATION),
                self.path.as_str(),
                Some(SESSION_INTERFACE),
                "SetLockedHint",
                &(locked,),
            )
            .await?;
        Ok(())
    }

    pub async fn monitor(
        &self,
        connection: &mut Connection,
    ) -> Result<Login1SessionMonitor, Login1Error> {
        Login1SessionMonitor::start(connection, self.clone()).await
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Login1SessionEvent {
    Ignored,
    Lock,
    Unlock,
    OwnerChanged,
}

/// One exact session signal rule plus the initial lock hint read after install.
pub struct Login1SessionMonitor {
    session: Login1Session,
    initially_locked: bool,
    rule: MatchRule,
}

impl Login1SessionMonitor {
    async fn start(
        connection: &mut Connection,
        session: Login1Session,
    ) -> Result<Self, Login1Error> {
        let mut rule = session_rule(session.path.as_str())?;
        connection.add_match(&mut rule).await?;
        let initially_locked = match session.locked_hint(connection).await {
            Ok(locked) => locked,
            Err(error) => {
                let _ = connection.remove_match(&rule).await;
                return Err(error);
            }
        };
        Ok(Self {
            session,
            initially_locked,
            rule,
        })
    }

    pub fn session(&self) -> &Login1Session {
        &self.session
    }

    pub const fn initially_locked(&self) -> bool {
        self.initially_locked
    }

    pub fn observe(&mut self, message: &Message) -> Result<Login1SessionEvent, Login1Error> {
        if self.rule.observe(message)? {
            return Ok(Login1SessionEvent::OwnerChanged);
        }
        if !self.rule.matches(message) {
            return Ok(Login1SessionEvent::Ignored);
        }
        classify_signal(message)
    }

    pub async fn set_locked_hint(
        &self,
        connection: &mut Connection,
        locked: bool,
    ) -> Result<(), Login1Error> {
        self.session.set_locked_hint(connection, locked).await
    }

    pub async fn close(self, connection: &mut Connection) -> TransportResult<()> {
        connection.remove_match(&self.rule).await
    }
}

pub async fn lock_sessions(connection: &mut Connection) -> Result<(), Login1Error> {
    let (): () = connection
        .call(
            Some(DESTINATION),
            MANAGER_PATH,
            Some(MANAGER_INTERFACE),
            "LockSessions",
            &(),
        )
        .await?;
    Ok(())
}

pub async fn suspend(connection: &mut Connection, interactive: bool) -> Result<(), Login1Error> {
    let (): () = connection
        .call(
            Some(DESTINATION),
            MANAGER_PATH,
            Some(MANAGER_INTERFACE),
            "Suspend",
            &(interactive,),
        )
        .await?;
    Ok(())
}

fn session_rule(path: &str) -> TransportResult<MatchRule> {
    MatchRule::signal(Some(DESTINATION), Some(path), Some(SESSION_INTERFACE), None)
}

fn classify_member(member: Option<&str>) -> Login1SessionEvent {
    match member {
        Some("Lock") => Login1SessionEvent::Lock,
        Some("Unlock") => Login1SessionEvent::Unlock,
        _ => Login1SessionEvent::Ignored,
    }
}

fn classify_signal(message: &Message) -> Result<Login1SessionEvent, Login1Error> {
    let event = classify_member(message.member());
    if matches!(event, Login1SessionEvent::Lock | Login1SessionEvent::Unlock) {
        message.body::<()>()?;
    }
    Ok(event)
}

#[derive(Debug, thiserror::Error)]
pub enum Login1Error {
    #[error(transparent)]
    Transport(#[from] Error),
    #[error("logind process id must be nonzero")]
    InvalidPid,
    #[error("logind session id must be nonempty")]
    InvalidSessionId,
    #[error("logind returned a non-session object path `{path}`")]
    UnexpectedSessionPath { path: String },
    #[error("logind session property `{property}` has the wrong D-Bus type: {source}")]
    InvalidProperty {
        property: &'static str,
        source: zvariant::Error,
    },
}

impl Login1Error {
    pub fn is_service_unavailable(&self) -> bool {
        match self {
            Self::Transport(Error::AddressUnavailable(_) | Error::Io(_)) => true,
            Self::Transport(Error::Method { name, .. }) => matches!(
                name.as_str(),
                "org.freedesktop.DBus.Error.NameHasNoOwner"
                    | "org.freedesktop.DBus.Error.ServiceUnknown"
                    | "org.freedesktop.DBus.Error.Spawn.ServiceNotFound"
            ),
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Serialize;
    use zvariant::DynamicType;

    use crate::{MessageKind, wire};

    fn session_signal<T>(member: &str, body: &T) -> Message
    where
        T: ?Sized + Serialize + DynamicType,
    {
        let encoded = wire::encode_outgoing(
            wire::Outgoing {
                kind: MessageKind::Signal,
                flags: 0,
                serial: 7,
                reply_serial: None,
                path: Some("/org/freedesktop/login1/session/_31"),
                interface: Some(SESSION_INTERFACE),
                member: Some(member),
                error_name: None,
                destination: None,
            },
            body,
        )
        .unwrap();
        wire::decode_message(encoded.bytes, Vec::new()).unwrap()
    }

    #[test]
    fn session_paths_are_constrained_to_login1_session_objects() {
        let valid = OwnedObjectPath::try_from("/org/freedesktop/login1/session/_31").unwrap();
        assert_eq!(
            Login1Session::from_path(valid).unwrap().path().as_str(),
            "/org/freedesktop/login1/session/_31"
        );

        for path in [
            "/org/freedesktop/login1",
            "/org/freedesktop/login1/session/_31/child",
            "/org/freedesktop/login1/user/_1000",
        ] {
            let path = OwnedObjectPath::try_from(path).unwrap();
            assert!(matches!(
                Login1Session::from_path(path),
                Err(Login1Error::UnexpectedSessionPath { .. })
            ));
        }
    }

    #[test]
    fn monitor_rule_is_exact_and_member_classification_is_closed() {
        let rule = session_rule("/org/freedesktop/login1/session/_31").unwrap();
        assert_eq!(
            rule.bus_expression(),
            "type='signal',sender='org.freedesktop.login1',path='/org/freedesktop/login1/session/_31',interface='org.freedesktop.login1.Session'"
        );
        assert_eq!(classify_member(Some("Lock")), Login1SessionEvent::Lock);
        assert_eq!(classify_member(Some("Unlock")), Login1SessionEvent::Unlock);
        assert_eq!(
            classify_member(Some("PauseDevice")),
            Login1SessionEvent::Ignored
        );
        assert_eq!(classify_member(None), Login1SessionEvent::Ignored);
    }

    #[test]
    fn lock_and_unlock_signals_require_an_empty_body() {
        assert_eq!(
            classify_signal(&session_signal("Lock", &())).unwrap(),
            Login1SessionEvent::Lock
        );
        assert_eq!(
            classify_signal(&session_signal("Unlock", &())).unwrap(),
            Login1SessionEvent::Unlock
        );
        assert!(matches!(
            classify_signal(&session_signal("Lock", &true)),
            Err(Login1Error::Transport(Error::InvalidMessage(_)))
        ));
        assert_eq!(
            classify_signal(&session_signal("PauseDevice", &true)).unwrap(),
            Login1SessionEvent::Ignored
        );
    }

    #[test]
    fn unavailable_login1_is_distinct_from_invalid_session_data() {
        let unavailable = Login1Error::Transport(Error::Method {
            name: "org.freedesktop.DBus.Error.ServiceUnknown".to_owned(),
            message: "not installed".to_owned(),
        });
        assert!(unavailable.is_service_unavailable());
        assert!(!Login1Error::InvalidPid.is_service_unavailable());
    }
}
