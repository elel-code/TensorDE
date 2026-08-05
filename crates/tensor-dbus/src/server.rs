use std::borrow::Cow;

use serde::{Serialize, de::DeserializeOwned};
use zvariant::{DynamicType, Type};

use crate::{Connection, Message, MessageKind, MethodCallFlags, Result};

pub type MethodResult<T> = std::result::Result<T, MethodError>;

/// A validated incoming method call without an additional body allocation.
pub struct MethodCall(Message);

impl MethodCall {
    pub fn new(message: Message) -> Option<Self> {
        if message.kind() == MessageKind::MethodCall {
            Some(Self(message))
        } else {
            None
        }
    }

    pub fn message(&self) -> &Message {
        &self.0
    }

    pub fn path(&self) -> &str {
        self.0
            .path()
            .expect("decoded method calls always carry an object path")
    }

    pub fn interface(&self) -> Option<&str> {
        self.0.interface()
    }

    pub fn member(&self) -> &str {
        self.0
            .member()
            .expect("decoded method calls always carry a member")
    }

    pub fn sender(&self) -> Option<&str> {
        self.0.sender()
    }

    pub fn expects_reply(&self) -> bool {
        self.0.expects_reply()
    }

    pub fn flags(&self) -> MethodCallFlags {
        self.0
            .method_call_flags()
            .expect("MethodCall always wraps a method-call message")
    }

    pub fn body<T>(&self) -> MethodResult<T>
    where
        T: DeserializeOwned + Type,
    {
        self.0.body().map_err(MethodError::invalid_args)
    }

    pub fn require_path(&self, expected: &str, message: impl Into<String>) -> MethodResult<()> {
        if self.path() == expected {
            Ok(())
        } else {
            Err(MethodError::unknown_object(message))
        }
    }

    pub fn require_interface(
        &self,
        expected: &str,
        message: impl Into<String>,
    ) -> MethodResult<()> {
        if self.interface() == Some(expected) {
            Ok(())
        } else {
            Err(MethodError::unknown_interface(message))
        }
    }
}

/// A standard string-bearing D-Bus method error.
#[derive(Debug, thiserror::Error)]
#[error("{name}: {message}")]
pub struct MethodError {
    name: Cow<'static, str>,
    message: String,
}

impl MethodError {
    pub fn new(name: impl Into<Cow<'static, str>>, message: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            message: message.into(),
        }
    }

    pub fn failed(message: impl Into<String>) -> Self {
        Self::new("org.freedesktop.DBus.Error.Failed", message)
    }

    pub fn access_denied(message: impl Into<String>) -> Self {
        Self::new("org.freedesktop.DBus.Error.AccessDenied", message)
    }

    pub fn invalid_args(error: impl std::fmt::Display) -> Self {
        Self::new("org.freedesktop.DBus.Error.InvalidArgs", error.to_string())
    }

    pub fn unknown_object(message: impl Into<String>) -> Self {
        Self::new("org.freedesktop.DBus.Error.UnknownObject", message)
    }

    pub fn unknown_interface(message: impl Into<String>) -> Self {
        Self::new("org.freedesktop.DBus.Error.UnknownInterface", message)
    }

    pub fn unknown_method(message: impl Into<String>) -> Self {
        Self::new("org.freedesktop.DBus.Error.UnknownMethod", message)
    }

    pub fn unknown_property(message: impl Into<String>) -> Self {
        Self::new("org.freedesktop.DBus.Error.UnknownProperty", message)
    }

    pub fn property_read_only(message: impl Into<String>) -> Self {
        Self::new("org.freedesktop.DBus.Error.PropertyReadOnly", message)
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

pub async fn reply_method_result<T>(
    connection: &mut Connection,
    call: &MethodCall,
    result: MethodResult<T>,
) -> Result<()>
where
    T: Serialize + DynamicType,
{
    match result {
        Ok(body) => connection.reply(call.message(), &body).await,
        Err(error) => reply_method_error(connection, call, error).await,
    }
}

pub async fn reply_method<T>(connection: &mut Connection, call: &MethodCall, body: &T) -> Result<()>
where
    T: ?Sized + Serialize + DynamicType,
{
    connection.reply(call.message(), body).await
}

pub async fn reply_method_error(
    connection: &mut Connection,
    call: &MethodCall,
    error: MethodError,
) -> Result<()> {
    connection
        .reply_error(call.message(), error.name(), error.message())
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_method_errors_keep_dbus_names() {
        let cases = [
            MethodError::failed("failed"),
            MethodError::access_denied("denied"),
            MethodError::invalid_args("invalid"),
            MethodError::unknown_object("object"),
            MethodError::unknown_interface("interface"),
            MethodError::unknown_method("method"),
            MethodError::unknown_property("property"),
            MethodError::property_read_only("read-only"),
        ];
        for error in cases {
            assert!(error.name().starts_with("org.freedesktop.DBus.Error."));
            assert!(!error.message().is_empty());
        }
    }
}
