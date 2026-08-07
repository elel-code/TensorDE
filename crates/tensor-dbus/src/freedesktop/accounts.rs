//! Typed, bounded access to AccountsService cached users.

use std::collections::HashMap;

use zvariant::{OwnedObjectPath, OwnedValue};

use crate::{Connection, Error, PendingReply};

pub const DESTINATION: &str = "org.freedesktop.Accounts";
pub const ROOT_PATH: &str = "/org/freedesktop/Accounts";
pub const ROOT_INTERFACE: &str = "org.freedesktop.Accounts";
pub const USER_INTERFACE: &str = "org.freedesktop.Accounts.User";
pub const PROPERTIES_INTERFACE: &str = "org.freedesktop.DBus.Properties";

type Properties = HashMap<String, OwnedValue>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AccountsUser {
    path: OwnedObjectPath,
    uid: u64,
    username: String,
    real_name: String,
    icon_file: String,
    system_account: bool,
    locked: bool,
}

impl AccountsUser {
    pub fn from_parts(
        path: OwnedObjectPath,
        uid: u64,
        username: String,
        real_name: String,
        icon_file: String,
        system_account: bool,
        locked: bool,
    ) -> Self {
        Self {
            path,
            uid,
            username,
            real_name,
            icon_file,
            system_account,
            locked,
        }
    }

    pub fn path(&self) -> &OwnedObjectPath {
        &self.path
    }

    pub const fn uid(&self) -> u64 {
        self.uid
    }

    pub fn username(&self) -> &str {
        &self.username
    }

    pub fn real_name(&self) -> &str {
        &self.real_name
    }

    pub fn display_name(&self) -> &str {
        if self.real_name.is_empty() {
            &self.username
        } else {
            &self.real_name
        }
    }

    pub fn icon_file(&self) -> Option<&str> {
        (!self.icon_file.is_empty()).then_some(self.icon_file.as_str())
    }

    pub const fn system_account(&self) -> bool {
        self.system_account
    }

    pub const fn locked(&self) -> bool {
        self.locked
    }

    pub const fn login_eligible(&self) -> bool {
        !self.system_account && !self.locked
    }
}

/// Fetches a bounded cached-user snapshot with every property request in flight.
///
/// The caller owns and drives `connection`. Replies are decoded in stable path
/// order, while D-Bus routing retains out-of-order replies without extra tasks.
pub async fn list_cached_users(
    connection: &mut Connection,
    maximum: usize,
) -> Result<Vec<AccountsUser>, AccountsError> {
    if maximum == 0 {
        return Err(AccountsError::InvalidLimit);
    }
    let paths: Vec<OwnedObjectPath> = connection
        .call(
            Some(DESTINATION),
            ROOT_PATH,
            Some(ROOT_INTERFACE),
            "ListCachedUsers",
            &(),
        )
        .await?;
    if paths.len() > maximum {
        return Err(AccountsError::UserLimit {
            count: paths.len(),
            maximum,
        });
    }

    let mut pending = Vec::with_capacity(paths.len());
    for path in paths {
        let reply = match connection
            .send_call::<_, Properties>(
                Some(DESTINATION),
                path.as_str(),
                Some(PROPERTIES_INTERFACE),
                "GetAll",
                &(USER_INTERFACE,),
            )
            .await
        {
            Ok(reply) => reply,
            Err(error) => {
                abandon_all(connection, pending);
                return Err(error.into());
            }
        };
        pending.push((path, reply));
    }

    let mut users = Vec::with_capacity(pending.len());
    let mut pending = pending.into_iter();
    while let Some((path, reply)) = pending.next() {
        let properties = match reply.wait(connection).await {
            Ok(properties) => properties,
            Err(error) => {
                abandon_all(connection, pending);
                return Err(error.into());
            }
        };
        match decode_user(path, &properties) {
            Ok(user) => users.push(user),
            Err(error) => {
                abandon_all(connection, pending);
                return Err(error);
            }
        }
    }
    Ok(users)
}

fn abandon_all(
    connection: &mut Connection,
    pending: impl IntoIterator<Item = (OwnedObjectPath, PendingReply<Properties>)>,
) {
    for (_, reply) in pending {
        let _ = reply.abandon(connection);
    }
}

fn decode_user(
    path: OwnedObjectPath,
    properties: &Properties,
) -> Result<AccountsUser, AccountsError> {
    Ok(AccountsUser {
        path,
        uid: required_u64(properties, "Uid")?,
        username: required_string(properties, "UserName")?,
        real_name: required_string(properties, "RealName")?,
        icon_file: required_string(properties, "IconFile")?,
        system_account: required_bool(properties, "SystemAccount")?,
        locked: required_bool(properties, "Locked")?,
    })
}

fn required<'a>(
    properties: &'a Properties,
    property: &'static str,
) -> Result<&'a OwnedValue, AccountsError> {
    properties
        .get(property)
        .ok_or(AccountsError::MissingProperty { property })
}

fn required_u64(properties: &Properties, property: &'static str) -> Result<u64, AccountsError> {
    u64::try_from(required(properties, property)?)
        .map_err(|source| AccountsError::InvalidProperty { property, source })
}

fn required_bool(properties: &Properties, property: &'static str) -> Result<bool, AccountsError> {
    bool::try_from(required(properties, property)?)
        .map_err(|source| AccountsError::InvalidProperty { property, source })
}

fn required_string(
    properties: &Properties,
    property: &'static str,
) -> Result<String, AccountsError> {
    <&str>::try_from(required(properties, property)?)
        .map(str::to_owned)
        .map_err(|source| AccountsError::InvalidProperty { property, source })
}

#[derive(Debug, thiserror::Error)]
pub enum AccountsError {
    #[error(transparent)]
    Transport(#[from] Error),
    #[error("AccountsService user limit must be nonzero")]
    InvalidLimit,
    #[error("AccountsService returned {count} cached users; maximum is {maximum}")]
    UserLimit { count: usize, maximum: usize },
    #[error("AccountsService user response omitted required property `{property}`")]
    MissingProperty { property: &'static str },
    #[error("AccountsService user property `{property}` has the wrong D-Bus type: {source}")]
    InvalidProperty {
        property: &'static str,
        source: zvariant::Error,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn properties(entries: impl IntoIterator<Item = (&'static str, OwnedValue)>) -> Properties {
        entries
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect()
    }

    fn user_properties() -> Properties {
        properties([
            ("Uid", 1000_u64.into()),
            ("UserName", OwnedValue::from(zvariant::Str::from("tensor"))),
            (
                "RealName",
                OwnedValue::from(zvariant::Str::from("Tensor User")),
            ),
            (
                "IconFile",
                OwnedValue::from(zvariant::Str::from("/var/lib/AccountsService/icons/tensor")),
            ),
            ("SystemAccount", false.into()),
            ("Locked", false.into()),
        ])
    }

    #[test]
    fn user_snapshot_is_typed_and_exposes_login_policy_inputs() {
        let user = decode_user(
            OwnedObjectPath::try_from("/org/freedesktop/Accounts/User1000").unwrap(),
            &user_properties(),
        )
        .unwrap();
        assert_eq!(user.uid(), 1000);
        assert_eq!(user.username(), "tensor");
        assert_eq!(user.display_name(), "Tensor User");
        assert!(user.login_eligible());
        assert!(user.icon_file().is_some());
    }

    #[test]
    fn empty_real_name_falls_back_to_username_and_locked_users_are_explicit() {
        let mut properties = user_properties();
        properties.insert(
            "RealName".to_owned(),
            OwnedValue::from(zvariant::Str::from("")),
        );
        properties.insert("Locked".to_owned(), true.into());
        let user = decode_user(
            OwnedObjectPath::try_from("/org/freedesktop/Accounts/User1000").unwrap(),
            &properties,
        )
        .unwrap();
        assert_eq!(user.display_name(), "tensor");
        assert!(!user.login_eligible());
    }

    #[test]
    fn missing_and_wrong_typed_properties_are_structured_errors() {
        let path = OwnedObjectPath::try_from("/org/freedesktop/Accounts/User1000").unwrap();
        let mut properties = user_properties();
        properties.remove("Uid");
        assert!(matches!(
            decode_user(path.clone(), &properties),
            Err(AccountsError::MissingProperty { property: "Uid" })
        ));
        properties.insert("Uid".to_owned(), true.into());
        assert!(matches!(
            decode_user(path, &properties),
            Err(AccountsError::InvalidProperty {
                property: "Uid",
                ..
            })
        ));
    }
}
