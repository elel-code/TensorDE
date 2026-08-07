use tensor_dbus::{
    Connection,
    freedesktop::accounts::{AccountsError, AccountsUser, list_cached_users},
};

use crate::{GreeterModelError, UserAccount};

/// Discover login-eligible users through a caller-owned Compio D-Bus connection.
pub async fn discover_users(maximum: usize) -> Result<Vec<UserAccount>, UserDiscoveryError> {
    let mut connection = Connection::system_bus().await?;
    let users = list_cached_users(&mut connection, maximum).await?;
    eligible_users(users)
}

fn eligible_users(users: Vec<AccountsUser>) -> Result<Vec<UserAccount>, UserDiscoveryError> {
    let mut users = users
        .into_iter()
        .filter(AccountsUser::login_eligible)
        .map(|account| UserAccount::new(account.username(), account.display_name()))
        .collect::<Result<Vec<_>, _>>()?;
    users.sort_by(|left, right| {
        left.display_name
            .to_lowercase()
            .cmp(&right.display_name.to_lowercase())
            .then_with(|| left.username.cmp(&right.username))
    });
    Ok(users)
}

#[derive(Debug, thiserror::Error)]
pub enum UserDiscoveryError {
    #[error(transparent)]
    Dbus(#[from] tensor_dbus::Error),
    #[error(transparent)]
    Accounts(#[from] AccountsError),
    #[error(transparent)]
    Model(#[from] GreeterModelError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use tensor_dbus::zvariant::OwnedObjectPath;

    fn account(
        uid: u64,
        username: &str,
        real_name: &str,
        system: bool,
        locked: bool,
    ) -> AccountsUser {
        AccountsUser::from_parts(
            OwnedObjectPath::try_from(format!("/org/freedesktop/Accounts/User{uid}")).unwrap(),
            uid,
            username.to_owned(),
            real_name.to_owned(),
            String::new(),
            system,
            locked,
        )
    }

    #[test]
    fn discovery_filters_non_login_accounts_and_sorts_display_names() {
        let users = eligible_users(vec![
            account(1002, "zoe", "Zoe", false, false),
            account(1000, "alice", "Alice", false, false),
            account(998, "daemon", "Daemon", true, false),
            account(1001, "locked", "Locked", false, true),
        ])
        .unwrap();

        assert_eq!(
            users
                .iter()
                .map(|user| user.username.as_str())
                .collect::<Vec<_>>(),
            ["alice", "zoe"]
        );
    }
}
