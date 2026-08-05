use std::marker::PhantomData;

use serde::de::DeserializeOwned;
use zvariant::Type;

use crate::{Connection, Error, Message, MessageKind, Result};

/// A typed reply expected for an already-sent method call.
///
/// The token owns no connection or task. Callers may keep several tokens in
/// flight and decide when to await or decode each reply on the owning runtime.
/// Dropping it does not cancel the remote call. Use [`Self::abandon`] when an
/// event loop no longer needs the reply so the connection can discard it.
#[derive(Debug)]
#[must_use = "an issued D-Bus method call still has a reply to route"]
pub struct PendingReply<R> {
    serial: u32,
    response: PhantomData<fn() -> R>,
}

impl<R> PendingReply<R> {
    pub(crate) const fn new(serial: u32) -> Self {
        Self {
            serial,
            response: PhantomData,
        }
    }

    pub const fn serial(&self) -> u32 {
        self.serial
    }

    pub fn matches(&self, message: &Message) -> bool {
        matches!(
            message.kind(),
            MessageKind::MethodReturn | MessageKind::Error
        ) && message.reply_serial() == Some(self.serial)
    }

    pub fn decode(self, message: Message) -> Result<R>
    where
        R: DeserializeOwned + Type,
    {
        if !self.matches(&message) {
            return Err(Error::InvalidMessage(format!(
                "reply serial {:?} does not match pending call {}",
                message.reply_serial(),
                self.serial
            )));
        }
        match message.kind() {
            MessageKind::MethodReturn => message.body(),
            MessageKind::Error => Err(message.method_error()),
            kind => Err(Error::InvalidMessage(format!(
                "message type {kind:?} cannot reply to a method call"
            ))),
        }
    }

    pub async fn wait(self, connection: &mut Connection) -> Result<R>
    where
        R: DeserializeOwned + Type,
    {
        let reply = self.wait_message(connection).await?;
        self.decode(reply)
    }

    /// Waits for the raw reply while retaining this token for later decoding
    /// or abandonment. This is the cancellation-safe form for caller-owned
    /// timeout/select combinators.
    pub async fn wait_message(&self, connection: &mut Connection) -> Result<Message> {
        connection
            .next_matching(|message| self.matches(message))
            .await
    }

    /// Stops routing this call's reply without cancelling the remote method.
    pub fn abandon(self, connection: &mut Connection) -> Result<()> {
        connection.abandon_reply(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_reply_is_only_a_serial_token() {
        assert_eq!(
            std::mem::size_of::<PendingReply<Vec<String>>>(),
            std::mem::size_of::<u32>()
        );
    }
}
