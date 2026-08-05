use std::borrow::Cow;

use serde::{Serialize, de::DeserializeOwned};
use zvariant::{DynamicType, Type};

use crate::{
    Connection, Error, MatchRule, MethodCallFlags, PendingReply, Result, SignalStream,
    name::{validate_bus_name, validate_interface_name},
};

/// Converts a bus target or optional peer header into retained proxy storage.
pub trait IntoProxyTarget<'target> {
    fn into_proxy_target(self) -> Option<Cow<'target, str>>;
}

impl<'target, T> IntoProxyTarget<'target> for Option<T>
where
    T: Into<Cow<'target, str>>,
{
    fn into_proxy_target(self) -> Option<Cow<'target, str>> {
        self.map(Into::into)
    }
}

impl<'target> IntoProxyTarget<'target> for &'target str {
    fn into_proxy_target(self) -> Option<Cow<'target, str>> {
        Some(self.into())
    }
}

impl<'target> IntoProxyTarget<'target> for String {
    fn into_proxy_target(self) -> Option<Cow<'target, str>> {
        Some(self.into())
    }
}

impl<'target> IntoProxyTarget<'target> for Cow<'target, str> {
    fn into_proxy_target(self) -> Option<Cow<'target, str>> {
        Some(self)
    }
}

/// A validated, allocation-stable method endpoint over a caller-owned connection.
pub struct Proxy<'connection, 'target> {
    connection: &'connection mut Connection,
    destination: Option<Cow<'target, str>>,
    path: Cow<'target, str>,
    interface: Option<Cow<'target, str>>,
}

impl<'connection, 'target> Proxy<'connection, 'target> {
    pub fn new(
        connection: &'connection mut Connection,
        destination: impl IntoProxyTarget<'target>,
        path: impl Into<Cow<'target, str>>,
        interface: impl IntoProxyTarget<'target>,
    ) -> Result<Self> {
        let destination = destination.into_proxy_target();
        let path = path.into();
        let interface = interface.into_proxy_target();
        if let Some(destination) = &destination {
            validate_bus_name(destination, "bus name")?;
        }
        zvariant::ObjectPath::try_from(path.as_ref()).map_err(|_| Error::InvalidName {
            kind: "object path",
            value: path.to_string(),
        })?;
        if let Some(interface) = &interface {
            validate_interface_name(interface, "interface name")?;
        }
        Ok(Self {
            connection,
            destination,
            path,
            interface,
        })
    }

    pub async fn call<B, R>(&mut self, member: &str, body: &B) -> Result<R>
    where
        B: ?Sized + Serialize + DynamicType,
        R: DeserializeOwned + Type,
    {
        let pending = self.send_call(member, body).await?;
        self.wait(pending).await
    }

    pub async fn call_with_flags<B, R>(
        &mut self,
        member: &str,
        flags: MethodCallFlags,
        body: &B,
    ) -> Result<R>
    where
        B: ?Sized + Serialize + DynamicType,
        R: DeserializeOwned + Type,
    {
        let pending = self.send_call_with_flags(member, flags, body).await?;
        self.wait(pending).await
    }

    /// Sends a method call without waiting, allowing several in-flight replies.
    pub async fn send_call<B, R>(&mut self, member: &str, body: &B) -> Result<PendingReply<R>>
    where
        B: ?Sized + Serialize + DynamicType,
    {
        self.send_call_with_flags(member, MethodCallFlags::default(), body)
            .await
    }

    pub async fn send_call_with_flags<B, R>(
        &mut self,
        member: &str,
        flags: MethodCallFlags,
        body: &B,
    ) -> Result<PendingReply<R>>
    where
        B: ?Sized + Serialize + DynamicType,
    {
        if flags.contains(MethodCallFlags::NO_REPLY_EXPECTED) {
            return Err(Error::InvalidCallFlags);
        }
        self.connection
            .send_call_validated_target(
                self.destination.as_deref(),
                &self.path,
                self.interface.as_deref(),
                member,
                flags,
                body,
            )
            .await
    }

    pub async fn send_no_reply<B>(&mut self, member: &str, body: &B) -> Result<()>
    where
        B: ?Sized + Serialize + DynamicType,
    {
        self.send_no_reply_with_flags(member, MethodCallFlags::default(), body)
            .await
    }

    pub async fn send_no_reply_with_flags<B>(
        &mut self,
        member: &str,
        flags: MethodCallFlags,
        body: &B,
    ) -> Result<()>
    where
        B: ?Sized + Serialize + DynamicType,
    {
        self.connection
            .send_no_reply_with_flags(
                self.destination.as_deref(),
                &self.path,
                self.interface.as_deref(),
                member,
                flags,
                body,
            )
            .await
    }

    pub async fn wait<R>(&mut self, pending: PendingReply<R>) -> Result<R>
    where
        R: DeserializeOwned + Type,
    {
        pending.wait(self.connection).await
    }

    /// Waits for a raw reply while retaining its token for decode or
    /// cancellation-safe timeout handling.
    pub async fn wait_message<R>(&mut self, pending: &PendingReply<R>) -> Result<crate::Message> {
        pending.wait_message(self.connection).await
    }

    /// Stops routing a pending reply without cancelling the remote method.
    pub fn abandon<R>(&mut self, pending: PendingReply<R>) -> Result<()> {
        pending.abandon(self.connection)
    }

    /// Installs the bus-side match before the triggering method is called.
    pub async fn subscribe(&mut self, member: &str) -> Result<MatchRule> {
        let destination = self
            .destination
            .as_deref()
            .ok_or(Error::BusOperationOnPeer)?;
        let mut rule = MatchRule::signal(
            Some(destination),
            Some(&self.path),
            self.interface.as_deref(),
            Some(member),
        )?;
        self.connection.add_match(&mut rule).await?;
        Ok(rule)
    }

    pub fn signal_stream<'proxy>(&'proxy mut self, rule: MatchRule) -> SignalStream<'proxy> {
        SignalStream::new(self.connection, rule)
    }
}
