use super::*;

pub(super) fn network_auth_store() -> &'static Mutex<HashMap<String, NetworkAuth>> {
    static STORE: OnceLock<Mutex<HashMap<String, NetworkAuth>>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(super) fn stored_network_auth_for_uri(uri: &str) -> Option<NetworkAuth> {
    let key = network_auth_key(uri).ok()?;
    network_auth_store()
        .lock()
        .expect("network auth store poisoned")
        .get(&key)
        .cloned()
}

pub(super) fn network_auth_key(uri: &str) -> Result<String, NetworkUrlError> {
    let normalized = normalize_network_uri(uri)?;
    if normalized == NETWORK_ROOT_URI {
        return Ok(normalized);
    }
    let (scheme, rest) = split_scheme(&normalized)?;
    let after_slashes = rest
        .strip_prefix("//")
        .ok_or_else(|| NetworkUrlError::MissingAuthority(scheme.to_string()))?;
    let (authority, path) = after_slashes
        .split_once('/')
        .map_or((after_slashes, ""), |(authority, path)| (authority, path));
    let path_without_tail = path
        .split(['?', '#'])
        .next()
        .unwrap_or(path)
        .trim_matches('/');
    let share_segment = path_without_tail
        .split('/')
        .find(|segment| !segment.is_empty());

    match (scheme, share_segment) {
        ("smb" | "nfs", Some(share)) => Ok(format!("{scheme}://{authority}/{share}/")),
        _ => Ok(format!("{scheme}://{authority}/")),
    }
}

pub(super) fn apply_network_auth_to_mount_operation(
    operation: &gio::MountOperation,
    auth: &NetworkAuth,
) {
    operation.set_anonymous(auth.anonymous);
    operation.set_username(auth.username.as_deref());
    operation.set_domain(auth.domain.as_deref());
    operation.set_password(auth.password.as_deref());
    operation.set_password_save(if auth.remember {
        gio::PasswordSave::ForSession
    } else {
        gio::PasswordSave::Never
    });
}

pub(super) fn network_auth_required_prompt(
    message: &str,
    default_user: &str,
    default_domain: &str,
) -> NetworkAuthPrompt {
    let mut parts = Vec::new();
    if let Some(message) = non_empty_string(message) {
        parts.push(message);
    }
    let default_username = non_empty_string(default_user);
    let default_domain = non_empty_string(default_domain);
    if let Some(default_user) = default_username.as_deref() {
        parts.push(format!("user: {default_user}"));
    }
    if let Some(default_domain) = default_domain.as_deref() {
        parts.push(format!("domain: {default_domain}"));
    }
    let message = if parts.is_empty() {
        "authentication required".to_string()
    } else {
        parts.join("; ")
    };
    NetworkAuthPrompt {
        message,
        default_username,
        default_domain,
    }
}

pub(super) fn network_gio_error(
    uri: &str,
    operation: &'static str,
    err: gio::glib::Error,
) -> NetworkScanError {
    if err.matches::<gio::IOErrorEnum>(gio::IOErrorEnum::Cancelled) {
        return NetworkScanError::Cancelled;
    }
    let message = err.to_string();
    let lower = message.to_ascii_lowercase();
    if lower.contains("password")
        || lower.contains("credential")
        || lower.contains("authentication")
        || lower.contains("permission denied")
    {
        NetworkScanError::AuthenticationRequired {
            uri: uri.to_string(),
            message,
            default_username: None,
            default_domain: None,
        }
    } else {
        NetworkScanError::Gio {
            uri: uri.to_string(),
            operation,
            message,
        }
    }
}
