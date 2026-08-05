use crate::{
    Connection, Error, Message, MessageKind, Result,
    name::{
        validate_bus_name, validate_interface_name, validate_member_name, validate_unique_name,
    },
};

const MAX_MATCH_RULE_LEN: usize = 1024;
const MAX_MATCH_ARG_INDEX: u8 = 63;

#[derive(Clone, Debug, Eq, PartialEq)]
enum PathMatch {
    Exact(String),
    Namespace(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MatchRule {
    sender: Option<String>,
    local_sender: Option<String>,
    path: Option<PathMatch>,
    interface: Option<String>,
    member: Option<String>,
    destination: Option<String>,
    args: Vec<(u8, String)>,
    arg_paths: Vec<(u8, String)>,
    arg0_namespace: Option<String>,
    bus_expression: String,
    owner_change_expression: Option<String>,
}

impl MatchRule {
    pub fn signal(
        sender: Option<&str>,
        path: Option<&str>,
        interface: Option<&str>,
        member: Option<&str>,
    ) -> Result<Self> {
        if let Some(value) = sender {
            validate_bus_name(value, "signal sender")?;
        }
        if let Some(value) = path {
            zvariant::ObjectPath::try_from(value).map_err(|_| Error::InvalidName {
                kind: "signal object path",
                value: value.to_owned(),
            })?;
        }
        if let Some(value) = interface {
            validate_interface_name(value, "signal interface")?;
        }
        if let Some(value) = member {
            validate_member_name(value, "signal member")?;
        }
        let owner_change_expression = sender
            .filter(|value| !value.starts_with(':'))
            .map(owner_change_expression);
        let mut rule = Self {
            sender: sender.map(str::to_owned),
            local_sender: sender
                .filter(|value| value.starts_with(':'))
                .map(str::to_owned),
            path: path.map(|value| PathMatch::Exact(value.to_owned())),
            interface: interface.map(str::to_owned),
            member: member.map(str::to_owned),
            destination: None,
            args: Vec::new(),
            arg_paths: Vec::new(),
            arg0_namespace: None,
            bus_expression: String::new(),
            owner_change_expression,
        };
        rule.rebuild_expression()?;
        Ok(rule)
    }

    pub fn path_namespace(mut self, path: &str) -> Result<Self> {
        validate_object_path(path, "signal path namespace")?;
        self.path = Some(PathMatch::Namespace(path.to_owned()));
        self.rebuild_expression()?;
        Ok(self)
    }

    pub fn destination(mut self, destination: &str) -> Result<Self> {
        validate_unique_name(destination, "signal destination")?;
        self.destination = Some(destination.to_owned());
        self.rebuild_expression()?;
        Ok(self)
    }

    pub fn arg(mut self, index: u8, value: impl Into<String>) -> Result<Self> {
        validate_arg_index(index)?;
        remove_arg(&mut self.arg_paths, index);
        if index == 0 {
            self.arg0_namespace = None;
        }
        insert_arg(&mut self.args, index, value.into());
        self.rebuild_expression()?;
        Ok(self)
    }

    pub fn arg_path(mut self, index: u8, path: &str) -> Result<Self> {
        validate_arg_index(index)?;
        remove_arg(&mut self.args, index);
        if index == 0 {
            self.arg0_namespace = None;
        }
        insert_arg(&mut self.arg_paths, index, path.to_owned());
        self.rebuild_expression()?;
        Ok(self)
    }

    pub fn arg0_namespace(mut self, namespace: &str) -> Result<Self> {
        validate_bus_namespace(namespace)?;
        remove_arg(&mut self.args, 0);
        remove_arg(&mut self.arg_paths, 0);
        self.arg0_namespace = Some(namespace.to_owned());
        self.rebuild_expression()?;
        Ok(self)
    }

    pub fn bus_expression(&self) -> &str {
        &self.bus_expression
    }

    /// Tests the message fields that can be evaluated without bus state.
    ///
    /// Unique senders are compared directly. A well-known sender is compared
    /// with the current unique owner learned by [`Connection::add_match`] and
    /// maintained by [`Self::observe`].
    pub fn matches(&self, message: &Message) -> bool {
        message.kind() == MessageKind::Signal
            && (self.sender.is_none() || self.local_sender.as_deref() == message.sender())
            && self
                .path
                .as_ref()
                .is_none_or(|value| path_matches(value, message.path()))
            && self
                .interface
                .as_deref()
                .is_none_or(|value| message.interface() == Some(value))
            && self
                .member
                .as_deref()
                .is_none_or(|value| message.member() == Some(value))
            && self
                .destination
                .as_deref()
                .is_none_or(|value| message.destination() == Some(value))
            && self.body_matches(message)
    }

    /// Applies a matching `NameOwnerChanged` signal to local sender routing.
    ///
    /// Caller-owned multi-rule loops should call this for each installed rule
    /// before testing [`Self::matches`]. It returns `true` when the message was
    /// an ownership update for this rule and should not be dispatched as the
    /// rule's application signal.
    pub fn observe(&mut self, message: &Message) -> Result<bool> {
        let Some(expected) = self.well_known_sender() else {
            return Ok(false);
        };
        if !is_name_owner_changed(message) {
            return Ok(false);
        }
        let (name, _old_owner, new_owner): (String, String, String) = message.body()?;
        if name != expected {
            return Ok(false);
        }
        if !new_owner.is_empty() {
            validate_unique_name(&new_owner, "signal sender owner")?;
        }
        self.local_sender = (!new_owner.is_empty()).then_some(new_owner);
        Ok(true)
    }

    pub(crate) fn well_known_sender(&self) -> Option<&str> {
        self.sender
            .as_deref()
            .filter(|value| !value.starts_with(':'))
    }

    pub(crate) fn set_owner(&mut self, owner: Option<String>) {
        self.local_sender = owner;
    }

    pub(crate) fn owner_change_expression(&self) -> Option<&str> {
        self.owner_change_expression.as_deref()
    }

    fn is_owner_change(&self, message: &Message) -> bool {
        let Some(expected) = self.well_known_sender() else {
            return false;
        };
        if !is_name_owner_changed(message) {
            return false;
        }
        message
            .body::<(String, String, String)>()
            .is_ok_and(|(name, _, _)| name == expected)
    }

    fn body_matches(&self, message: &Message) -> bool {
        if self.args.is_empty() && self.arg_paths.is_empty() && self.arg0_namespace.is_none() {
            return true;
        }
        let Ok(Some(matches)) = message.inspect_body_structure(|body| {
            let fields = body.fields();
            if self.args.iter().any(|(index, expected)| {
                fields
                    .get(*index as usize)
                    .and_then(|value| <&str>::try_from(value).ok())
                    != Some(expected.as_str())
            }) {
                return false;
            }
            if self.arg_paths.iter().any(|(index, expected)| {
                fields
                    .get(*index as usize)
                    .is_none_or(|value| !argument_path_value_matches(expected, value))
            }) {
                return false;
            }
            self.arg0_namespace.as_deref().is_none_or(|namespace| {
                fields
                    .first()
                    .and_then(|value| <&str>::try_from(value).ok())
                    .is_some_and(|value| namespace_matches(namespace, value))
            })
        }) else {
            return false;
        };
        matches
    }

    fn rebuild_expression(&mut self) -> Result<()> {
        let mut expression = String::from("type='signal'");
        push_term(&mut expression, "sender", self.sender.as_deref());
        match self.path.as_ref() {
            Some(PathMatch::Exact(path)) => push_term(&mut expression, "path", Some(path)),
            Some(PathMatch::Namespace(path)) => {
                push_term(&mut expression, "path_namespace", Some(path));
            }
            None => {}
        }
        push_term(&mut expression, "interface", self.interface.as_deref());
        push_term(&mut expression, "member", self.member.as_deref());
        push_term(&mut expression, "destination", self.destination.as_deref());
        for (index, value) in &self.args {
            push_term(&mut expression, &format!("arg{index}"), Some(value));
        }
        for (index, value) in &self.arg_paths {
            push_term(&mut expression, &format!("arg{index}path"), Some(value));
        }
        push_term(
            &mut expression,
            "arg0namespace",
            self.arg0_namespace.as_deref(),
        );
        if expression.len() > MAX_MATCH_RULE_LEN {
            return Err(Error::InvalidMatchRule(format!(
                "rule exceeds {MAX_MATCH_RULE_LEN} bytes"
            )));
        }
        self.bus_expression = expression;
        Ok(())
    }
}

/// An async signal receiver borrowing the connection that drives its I/O.
#[must_use = "dropping a signal stream leaves its bus-side match installed until disconnect"]
pub struct SignalStream<'connection> {
    connection: &'connection mut Connection,
    rule: MatchRule,
}

impl<'connection> SignalStream<'connection> {
    pub(crate) fn new(connection: &'connection mut Connection, rule: MatchRule) -> Self {
        Self { connection, rule }
    }

    pub async fn next(&mut self) -> Result<Message> {
        loop {
            let message = self
                .connection
                .next_matching(|message| {
                    self.rule.matches(message) || self.rule.is_owner_change(message)
                })
                .await?;
            if self.rule.observe(&message)? {
                continue;
            }
            return Ok(message);
        }
    }

    /// Removes the bus-side match and returns the borrowed connection.
    pub async fn close(self) -> Result<&'connection mut Connection> {
        self.connection.remove_match(&self.rule).await?;
        Ok(self.connection)
    }
}

fn is_name_owner_changed(message: &Message) -> bool {
    message.kind() == MessageKind::Signal
        && message.sender() == Some("org.freedesktop.DBus")
        && message.path() == Some("/org/freedesktop/DBus")
        && message.interface() == Some("org.freedesktop.DBus")
        && message.member() == Some("NameOwnerChanged")
}

fn owner_change_expression(name: &str) -> String {
    let mut expression = String::from(
        "type='signal',sender='org.freedesktop.DBus',path='/org/freedesktop/DBus',interface='org.freedesktop.DBus',member='NameOwnerChanged'",
    );
    push_term(&mut expression, "arg0", Some(name));
    expression
}

fn push_term(expression: &mut String, key: &str, value: Option<&str>) {
    let Some(value) = value else {
        return;
    };
    expression.push(',');
    expression.push_str(key);
    expression.push_str("='");
    for character in value.chars() {
        if character == '\'' {
            expression.push_str("'\\''");
        } else {
            expression.push(character);
        }
    }
    expression.push('\'');
}

fn insert_arg(arguments: &mut Vec<(u8, String)>, index: u8, value: String) {
    match arguments.binary_search_by_key(&index, |(index, _)| *index) {
        Ok(position) => arguments[position].1 = value,
        Err(position) => arguments.insert(position, (index, value)),
    }
}

fn remove_arg(arguments: &mut Vec<(u8, String)>, index: u8) {
    if let Ok(position) = arguments.binary_search_by_key(&index, |(index, _)| *index) {
        arguments.remove(position);
    }
}

fn validate_arg_index(index: u8) -> Result<()> {
    if index <= MAX_MATCH_ARG_INDEX {
        Ok(())
    } else {
        Err(Error::InvalidMatchRule(format!(
            "argument index {index} exceeds {MAX_MATCH_ARG_INDEX}"
        )))
    }
}

fn validate_object_path(path: &str, kind: &'static str) -> Result<()> {
    zvariant::ObjectPath::try_from(path).map_err(|_| Error::InvalidName {
        kind,
        value: path.to_owned(),
    })?;
    Ok(())
}

fn validate_bus_namespace(namespace: &str) -> Result<()> {
    let (unique, value) = namespace
        .strip_prefix(':')
        .map_or((false, namespace), |value| (true, value));
    let valid = !value.is_empty()
        && namespace.len() <= 255
        && value.split('.').all(|element| {
            element.as_bytes().first().is_some_and(|first| {
                (unique || !first.is_ascii_digit())
                    && (first.is_ascii_alphanumeric() || matches!(first, b'_' | b'-'))
            }) && element
                .as_bytes()
                .iter()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        });
    if valid {
        Ok(())
    } else {
        Err(Error::InvalidMatchRule(format!(
            "invalid argument-zero namespace `{namespace}`"
        )))
    }
}

fn path_matches(expected: &PathMatch, actual: Option<&str>) -> bool {
    let Some(actual) = actual else {
        return false;
    };
    match expected {
        PathMatch::Exact(expected) => actual == expected,
        PathMatch::Namespace(expected) => {
            actual == expected
                || expected == "/"
                || actual
                    .strip_prefix(expected)
                    .is_some_and(|suffix| suffix.starts_with('/'))
        }
    }
}

fn namespace_matches(expected: &str, actual: &str) -> bool {
    actual == expected
        || actual
            .strip_prefix(expected)
            .is_some_and(|suffix| suffix.starts_with('.'))
}

fn argument_path_value_matches(expected: &str, value: &zvariant::Value<'_>) -> bool {
    if let Ok(actual) = <&str>::try_from(value) {
        return argument_path_matches(expected, actual);
    }
    zvariant::ObjectPath::try_from(value)
        .is_ok_and(|actual| argument_path_matches(expected, actual.as_str()))
}

fn argument_path_matches(expected: &str, actual: &str) -> bool {
    expected == actual
        || expected.ends_with('/') && actual.starts_with(expected)
        || actual.ends_with('/') && expected.starts_with(actual)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn match_rule_is_valid_dbus_syntax() {
        let rule = MatchRule::signal(
            Some("org.freedesktop.systemd1"),
            Some("/org/freedesktop/systemd1"),
            Some("org.freedesktop.systemd1.Manager"),
            Some("JobRemoved"),
        )
        .unwrap();
        assert_eq!(
            rule.bus_expression(),
            "type='signal',sender='org.freedesktop.systemd1',path='/org/freedesktop/systemd1',interface='org.freedesktop.systemd1.Manager',member='JobRemoved'"
        );
    }

    #[test]
    fn unique_sender_is_part_of_local_matching() {
        let rule = MatchRule::signal(Some(":1.7"), None, None, None).unwrap();
        assert_eq!(rule.local_sender.as_deref(), Some(":1.7"));

        let unresolved = MatchRule::signal(Some("org.tensor.Service"), None, None, None).unwrap();
        assert_eq!(unresolved.local_sender, None);
        assert!(unresolved.owner_change_expression().is_some());
    }

    #[test]
    fn extended_signal_rules_are_validated_sorted_and_escaped() {
        let rule = MatchRule::signal(None, None, Some("org.tensor.Signal"), Some("Changed"))
            .unwrap()
            .path_namespace("/org/tensor")
            .unwrap()
            .destination(":1.42")
            .unwrap()
            .arg(3, "third")
            .unwrap()
            .arg(2, "it's")
            .unwrap()
            .arg_path(1, "/org/tensor/Object")
            .unwrap()
            .arg0_namespace("org.tensor")
            .unwrap();
        assert_eq!(
            rule.bus_expression(),
            "type='signal',path_namespace='/org/tensor',interface='org.tensor.Signal',member='Changed',destination=':1.42',arg2='it'\\''s',arg3='third',arg1path='/org/tensor/Object',arg0namespace='org.tensor'"
        );
        assert!(
            MatchRule::signal(None, None, None, None)
                .unwrap()
                .arg(64, "invalid")
                .is_err()
        );
        assert!(
            MatchRule::signal(None, None, None, None)
                .unwrap()
                .arg0_namespace("org..tensor")
                .is_err()
        );
        assert!(
            MatchRule::signal(None, None, None, None)
                .unwrap()
                .arg(0, "x".repeat(MAX_MATCH_RULE_LEN))
                .is_err()
        );
    }
}
