use std::{
    collections::{HashSet, VecDeque},
    os::fd::AsFd,
};

use compio::net::UnixStream;
use serde::{Serialize, de::DeserializeOwned};
use zvariant::{DynamicType, Type};

use crate::{
    BusAddress, BusKind, Error, Guid, Message, MessageKind, MethodCallFlags, PendingReply, Result,
    auth,
    name::{
        validate_bus_name, validate_error_name, validate_interface_name, validate_member_name,
        validate_unique_name,
    },
    wire::{EncodedMessage, MethodCall, Outgoing, encode_method_call, encode_outgoing},
};

mod bus;
mod reader;
mod writer;

use reader::MessageReader;
use writer::MessageWriter;

const MAX_PENDING_MESSAGES: usize = 256;
const MAX_PENDING_BYTES: usize = 32 * 1024 * 1024;
const MAX_ABANDONED_REPLIES: usize = 256;

struct AbandonedReplies {
    serials: HashSet<u32>,
}

impl AbandonedReplies {
    fn new() -> Self {
        Self {
            serials: HashSet::with_capacity(MAX_ABANDONED_REPLIES),
        }
    }

    fn register(&mut self, serial: u32) -> Result<()> {
        if self.serials.len() == MAX_ABANDONED_REPLIES {
            return Err(Error::AbandonedReplyQueueFull {
                limit: MAX_ABANDONED_REPLIES,
            });
        }
        let inserted = self.serials.insert(serial);
        debug_assert!(inserted, "a pending reply token must be consumed once");
        Ok(())
    }

    fn take(&mut self, serial: u32) -> bool {
        if self.serials.is_empty() {
            return false;
        }
        self.serials.remove(&serial)
    }
}

/// A single-owner asynchronous connection to a D-Bus message bus.
///
/// All operations run on the caller's current Compio runtime. The connection
/// does not spawn tasks, threads, or a private executor.
pub struct Connection {
    reader: MessageReader,
    writer: MessageWriter,
    next_serial: u32,
    pending: VecDeque<Message>,
    pending_bytes: usize,
    abandoned_replies: AbandonedReplies,
    routing_failed: bool,
    mode: ConnectionMode,
    peer_credentials: Option<PeerCredentials>,
    server_guid: Guid,
    unique_name: Option<String>,
    unix_fd: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConnectionMode {
    Bus,
    Peer,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PeerCredentials {
    pub process_id: u32,
    pub user_id: u32,
    pub group_id: u32,
}

impl Connection {
    pub async fn session_bus() -> Result<Self> {
        Self::connect_bus(BusAddress::for_kind(BusKind::Session)?).await
    }

    pub async fn system_bus() -> Result<Self> {
        Self::connect_bus(BusAddress::for_kind(BusKind::System)?).await
    }

    pub async fn connect_bus(address: BusAddress) -> Result<Self> {
        let mut last_error = None;
        for endpoint in address.endpoints() {
            match Self::connect_bus_endpoint(endpoint).await {
                Ok(connection) => return Ok(connection),
                Err(error) => last_error = Some(error),
            }
        }
        Err(last_error.expect("validated D-Bus addresses contain a Unix endpoint"))
    }

    async fn connect_bus_endpoint(endpoint: &crate::address::BusEndpoint) -> Result<Self> {
        let mut stream = UnixStream::connect(endpoint.path()).await?;
        let authenticated = auth::authenticate_client(&mut stream, endpoint.guid()).await?;
        let mut connection =
            Self::from_authenticated(stream, ConnectionMode::Bus, None, authenticated);
        let unique_name: String = connection
            .call(
                Some("org.freedesktop.DBus"),
                "/org/freedesktop/DBus",
                Some("org.freedesktop.DBus"),
                "Hello",
                &(),
            )
            .await?;
        validate_unique_name(&unique_name, "unique bus name")?;
        connection.unique_name = Some(unique_name);
        Ok(connection)
    }

    pub async fn connect_peer(stream: UnixStream, expected_guid: Option<Guid>) -> Result<Self> {
        let mut stream = stream;
        let authenticated = auth::authenticate_client(&mut stream, expected_guid).await?;
        Ok(Self::from_authenticated(
            stream,
            ConnectionMode::Peer,
            None,
            authenticated,
        ))
    }

    pub async fn accept_peer(stream: UnixStream, server_guid: Guid) -> Result<Self> {
        let credentials = rustix::net::sockopt::socket_peercred(stream.as_fd())
            .map_err(|error| Error::Io(error.into()))?;
        let peer_credentials = PeerCredentials {
            process_id: credentials.pid.as_raw_pid() as u32,
            user_id: credentials.uid.as_raw(),
            group_id: credentials.gid.as_raw(),
        };
        Self::accept_peer_with_credentials(stream, server_guid, peer_credentials).await
    }

    pub(crate) async fn accept_peer_with_credentials(
        stream: UnixStream,
        server_guid: Guid,
        peer_credentials: PeerCredentials,
    ) -> Result<Self> {
        let mut stream = stream;
        let authenticated =
            auth::authenticate_server(&mut stream, server_guid, peer_credentials.user_id).await?;
        Ok(Self::from_authenticated(
            stream,
            ConnectionMode::Peer,
            Some(peer_credentials),
            authenticated,
        ))
    }

    fn from_authenticated(
        stream: UnixStream,
        mode: ConnectionMode,
        peer_credentials: Option<PeerCredentials>,
        authenticated: auth::Authenticated,
    ) -> Self {
        let auth::Authenticated {
            unix_fd,
            server_guid,
            buffered,
            unix_fds,
        } = authenticated;
        Self {
            reader: MessageReader::new(stream.clone(), unix_fd, buffered, unix_fds),
            writer: MessageWriter::new(stream, unix_fd),
            next_serial: 1,
            pending: VecDeque::with_capacity(16),
            pending_bytes: 0,
            abandoned_replies: AbandonedReplies::new(),
            routing_failed: false,
            mode,
            peer_credentials,
            server_guid,
            unique_name: None,
            unix_fd,
        }
    }

    pub const fn mode(&self) -> ConnectionMode {
        self.mode
    }

    pub fn unique_name(&self) -> Option<&str> {
        self.unique_name.as_deref()
    }

    pub const fn peer_credentials(&self) -> Option<PeerCredentials> {
        self.peer_credentials
    }

    pub const fn server_guid(&self) -> Guid {
        self.server_guid
    }

    pub const fn supports_unix_fd(&self) -> bool {
        self.unix_fd
    }

    /// Returns whether no terminal transport or protocol failure has occurred.
    pub fn is_usable(&self) -> bool {
        !self.routing_failed && !self.reader.is_failed() && !self.writer.is_failed()
    }

    /// Completes a write operation whose outer future was cancelled.
    ///
    /// Normal send APIs are already flushed before they return. This method is
    /// useful when caller-owned select/timeout logic drops a send future after
    /// it has started.
    pub async fn flush(&mut self) -> Result<()> {
        self.ensure_usable()?;
        self.writer.flush().await
    }

    /// Finishes any retained write, cancels a retained read, and closes the socket.
    ///
    /// This consumes the connection, discarding queued messages and pending
    /// routing state. Dropping the returned future aborts shutdown by dropping
    /// the connection; callers that do not need to finish an interrupted write
    /// can drop `Connection` directly.
    pub async fn close(self) -> Result<()> {
        let Self { reader, writer, .. } = self;
        drop(reader);
        writer.close().await
    }

    pub async fn call<'target, B, R>(
        &mut self,
        destination: impl Into<Option<&'target str>>,
        path: &str,
        interface: impl Into<Option<&'target str>>,
        member: &str,
        body: &B,
    ) -> Result<R>
    where
        B: ?Sized + Serialize + DynamicType,
        R: DeserializeOwned + Type,
    {
        let destination = destination.into();
        let interface = interface.into();
        let pending = self
            .send_call(destination, path, interface, member, body)
            .await?;
        pending.wait(self).await
    }

    /// Calls a method with standard bus and authorization flags.
    ///
    /// `NO_REPLY_EXPECTED` is rejected because this API must produce a typed
    /// reply. Use [`Self::send_no_reply_with_flags`] for that flag.
    pub async fn call_with_flags<'target, B, R>(
        &mut self,
        destination: impl Into<Option<&'target str>>,
        path: &str,
        interface: impl Into<Option<&'target str>>,
        member: &str,
        flags: MethodCallFlags,
        body: &B,
    ) -> Result<R>
    where
        B: ?Sized + Serialize + DynamicType,
        R: DeserializeOwned + Type,
    {
        let destination = destination.into();
        let interface = interface.into();
        let pending = self
            .send_call_with_flags(destination, path, interface, member, flags, body)
            .await?;
        pending.wait(self).await
    }

    /// Sends a method call and returns its typed reply token without waiting.
    pub async fn send_call<'target, B, R>(
        &mut self,
        destination: impl Into<Option<&'target str>>,
        path: &str,
        interface: impl Into<Option<&'target str>>,
        member: &str,
        body: &B,
    ) -> Result<PendingReply<R>>
    where
        B: ?Sized + Serialize + DynamicType,
    {
        let destination = destination.into();
        let interface = interface.into();
        self.send_call_with_flags(
            destination,
            path,
            interface,
            member,
            MethodCallFlags::default(),
            body,
        )
        .await
    }

    /// Sends a reply-producing method call with standard flags.
    pub async fn send_call_with_flags<'target, B, R>(
        &mut self,
        destination: impl Into<Option<&'target str>>,
        path: &str,
        interface: impl Into<Option<&'target str>>,
        member: &str,
        flags: MethodCallFlags,
        body: &B,
    ) -> Result<PendingReply<R>>
    where
        B: ?Sized + Serialize + DynamicType,
    {
        let destination = destination.into();
        let interface = interface.into();
        if flags.contains(MethodCallFlags::NO_REPLY_EXPECTED) {
            return Err(Error::InvalidCallFlags);
        }
        validate_target(destination, path, interface)?;
        self.send_call_validated_target(destination, path, interface, member, flags, body)
            .await
    }

    /// Sends a method call whose remote side must not return a reply.
    pub async fn send_no_reply<'target, B>(
        &mut self,
        destination: impl Into<Option<&'target str>>,
        path: &str,
        interface: impl Into<Option<&'target str>>,
        member: &str,
        body: &B,
    ) -> Result<()>
    where
        B: ?Sized + Serialize + DynamicType,
    {
        let destination = destination.into();
        let interface = interface.into();
        self.send_no_reply_with_flags(
            destination,
            path,
            interface,
            member,
            MethodCallFlags::default(),
            body,
        )
        .await
    }

    /// Sends a no-reply method call with additional standard flags.
    pub async fn send_no_reply_with_flags<'target, B>(
        &mut self,
        destination: impl Into<Option<&'target str>>,
        path: &str,
        interface: impl Into<Option<&'target str>>,
        member: &str,
        flags: MethodCallFlags,
        body: &B,
    ) -> Result<()>
    where
        B: ?Sized + Serialize + DynamicType,
    {
        let destination = destination.into();
        let interface = interface.into();
        validate_target(destination, path, interface)?;
        validate_member(member)?;
        let serial = self.allocate_serial()?;
        let flags =
            flags.without(MethodCallFlags::NO_REPLY_EXPECTED) | MethodCallFlags::NO_REPLY_EXPECTED;
        let bytes = encode_method_call(
            MethodCall {
                serial,
                flags,
                destination,
                path,
                interface,
                member,
            },
            body,
        )?;
        self.write_message(bytes).await
    }

    /// Stops routing a reply to a previously issued call.
    ///
    /// This is local bookkeeping only: the remote method keeps running. A reply
    /// already in the pending queue is removed immediately; a later reply is
    /// discarded when the caller next drives this connection.
    pub fn abandon_reply<R>(&mut self, pending: PendingReply<R>) -> Result<()> {
        self.ensure_usable()?;
        let serial = pending.serial();
        if let Some(index) = self
            .pending
            .iter()
            .position(|message| pending.matches(message))
        {
            self.remove_pending(index);
            return Ok(());
        }
        self.abandoned_replies.register(serial).inspect_err(|_| {
            self.routing_failed = true;
        })
    }

    pub(crate) async fn send_call_validated_target<B, R>(
        &mut self,
        destination: Option<&str>,
        path: &str,
        interface: Option<&str>,
        member: &str,
        flags: MethodCallFlags,
        body: &B,
    ) -> Result<PendingReply<R>>
    where
        B: ?Sized + Serialize + DynamicType,
    {
        validate_member(member)?;
        let serial = self.allocate_serial()?;
        let bytes = encode_method_call(
            MethodCall {
                serial,
                flags,
                destination,
                path,
                interface,
                member,
            },
            body,
        )?;
        self.write_message(bytes).await?;
        Ok(PendingReply::new(serial))
    }

    /// Receives the next queued or socket message.
    pub async fn receive(&mut self) -> Result<Message> {
        self.ensure_usable()?;
        loop {
            let message = match self.pop_pending() {
                Some(message) => message,
                None => self.read_message().await?,
            };
            if !self.discard_if_abandoned_reply(&message) {
                return Ok(message);
            }
        }
    }

    /// Sends a method return to an incoming call.
    pub async fn reply<B>(&mut self, call: &Message, body: &B) -> Result<()>
    where
        B: ?Sized + Serialize + DynamicType,
    {
        self.validate_incoming_call(call)?;
        if !call.expects_reply() {
            return Ok(());
        }
        let serial = self.allocate_serial()?;
        self.send_outgoing(
            Outgoing {
                kind: MessageKind::MethodReturn,
                flags: 0,
                serial,
                reply_serial: Some(call.serial()),
                path: None,
                interface: None,
                member: None,
                error_name: None,
                destination: call.sender(),
            },
            body,
        )
        .await
    }

    /// Sends a standard string-bearing D-Bus error reply.
    pub async fn reply_error(
        &mut self,
        call: &Message,
        error_name: &str,
        message: &str,
    ) -> Result<()> {
        self.validate_incoming_call(call)?;
        validate_error_name(error_name, "error name")?;
        if !call.expects_reply() {
            return Ok(());
        }
        let serial = self.allocate_serial()?;
        self.send_outgoing(
            Outgoing {
                kind: MessageKind::Error,
                flags: 0,
                serial,
                reply_serial: Some(call.serial()),
                path: None,
                interface: None,
                member: None,
                error_name: Some(error_name),
                destination: call.sender(),
            },
            message,
        )
        .await
    }

    /// Broadcasts a signal from this connection.
    pub async fn emit_signal<B>(
        &mut self,
        path: &str,
        interface: &str,
        member: &str,
        body: &B,
    ) -> Result<()>
    where
        B: ?Sized + Serialize + DynamicType,
    {
        self.emit_signal_inner(None, path, interface, member, body)
            .await
    }

    /// Emits a signal routed only to the named connection or bus owner.
    pub async fn emit_signal_to<B>(
        &mut self,
        destination: &str,
        path: &str,
        interface: &str,
        member: &str,
        body: &B,
    ) -> Result<()>
    where
        B: ?Sized + Serialize + DynamicType,
    {
        validate_bus_name(destination, "destination bus name")?;
        self.emit_signal_inner(Some(destination), path, interface, member, body)
            .await
    }

    async fn emit_signal_inner<B>(
        &mut self,
        destination: Option<&str>,
        path: &str,
        interface: &str,
        member: &str,
        body: &B,
    ) -> Result<()>
    where
        B: ?Sized + Serialize + DynamicType,
    {
        validate_signal(path, interface, member)?;
        let serial = self.allocate_serial()?;
        self.send_outgoing(
            Outgoing {
                kind: MessageKind::Signal,
                flags: 0,
                serial,
                reply_serial: None,
                path: Some(path),
                interface: Some(interface),
                member: Some(member),
                error_name: None,
                destination,
            },
            body,
        )
        .await
    }

    pub(crate) async fn next_matching(
        &mut self,
        predicate: impl Fn(&Message) -> bool,
    ) -> Result<Message> {
        self.ensure_usable()?;
        loop {
            self.discard_abandoned_pending();
            if let Some(index) = self.pending.iter().position(&predicate) {
                return Ok(self.remove_pending(index));
            }
            let message = self.read_message().await?;
            if self.discard_if_abandoned_reply(&message) {
                continue;
            }
            if predicate(&message) {
                return Ok(message);
            }
            self.queue_pending(message)?;
        }
    }

    fn queue_pending(&mut self, message: Message) -> Result<()> {
        if self.pending.len() == MAX_PENDING_MESSAGES {
            self.routing_failed = true;
            return Err(Error::PendingQueueFull {
                limit: MAX_PENDING_MESSAGES,
            });
        }
        let Some(pending_bytes) = self.pending_bytes.checked_add(message.wire_len()) else {
            self.routing_failed = true;
            return Err(Error::PendingBytesFull {
                limit: MAX_PENDING_BYTES,
            });
        };
        if pending_bytes > MAX_PENDING_BYTES {
            self.routing_failed = true;
            return Err(Error::PendingBytesFull {
                limit: MAX_PENDING_BYTES,
            });
        }
        self.pending_bytes = pending_bytes;
        self.pending.push_back(message);
        Ok(())
    }

    fn allocate_serial(&mut self) -> Result<u32> {
        let serial = self.next_serial;
        if serial == 0 {
            return Err(Error::SerialExhausted);
        }
        self.next_serial = serial.checked_add(1).unwrap_or(0);
        Ok(serial)
    }

    fn discard_abandoned_pending(&mut self) {
        if self.abandoned_replies.serials.is_empty() {
            return;
        }
        let mut index = 0;
        while index < self.pending.len() {
            let reply_serial = self.pending[index].reply_serial();
            if reply_serial.is_some_and(|serial| self.abandoned_replies.take(serial)) {
                self.remove_pending(index);
            } else {
                index += 1;
            }
        }
    }

    fn discard_if_abandoned_reply(&mut self, message: &Message) -> bool {
        message
            .reply_serial()
            .is_some_and(|serial| self.abandoned_replies.take(serial))
    }

    fn pop_pending(&mut self) -> Option<Message> {
        let message = self.pending.pop_front()?;
        self.pending_bytes -= message.wire_len();
        Some(message)
    }

    fn remove_pending(&mut self, index: usize) -> Message {
        let message = self.pending.remove(index).unwrap();
        self.pending_bytes -= message.wire_len();
        message
    }

    fn validate_incoming_call(&self, call: &Message) -> Result<()> {
        if call.kind() != MessageKind::MethodCall {
            return Err(Error::InvalidMessage(
                "only a method call can receive a reply".to_owned(),
            ));
        }
        if self.mode == ConnectionMode::Bus && call.sender().is_none() {
            return Err(Error::InvalidMessage(
                "incoming bus method call has no sender".to_owned(),
            ));
        }
        Ok(())
    }

    async fn send_outgoing<B>(&mut self, outgoing: Outgoing<'_>, body: &B) -> Result<()>
    where
        B: ?Sized + Serialize + DynamicType,
    {
        self.write_message(encode_outgoing(outgoing, body)?).await
    }

    async fn write_message(&mut self, encoded: EncodedMessage) -> Result<()> {
        self.ensure_usable()?;
        self.writer.write(encoded).await
    }

    async fn read_message(&mut self) -> Result<Message> {
        self.ensure_usable()?;
        self.reader.read().await
    }

    fn ensure_usable(&self) -> Result<()> {
        if self.is_usable() {
            Ok(())
        } else {
            Err(Error::ConnectionUnusable)
        }
    }

    fn ensure_bus(&self) -> Result<()> {
        if self.mode == ConnectionMode::Bus {
            Ok(())
        } else {
            Err(Error::BusOperationOnPeer)
        }
    }
}

fn validate_signal(path: &str, interface: &str, member: &str) -> Result<()> {
    zvariant::ObjectPath::try_from(path).map_err(|_| Error::InvalidName {
        kind: "object path",
        value: path.to_owned(),
    })?;
    validate_interface_name(interface, "interface name")?;
    validate_member_name(member, "member name")?;
    Ok(())
}

fn validate_target(destination: Option<&str>, path: &str, interface: Option<&str>) -> Result<()> {
    if let Some(destination) = destination {
        validate_bus_name(destination, "bus name")?;
    }
    zvariant::ObjectPath::try_from(path).map_err(|_| Error::InvalidName {
        kind: "object path",
        value: path.to_owned(),
    })?;
    if let Some(interface) = interface {
        validate_interface_name(interface, "interface name")?;
    }
    Ok(())
}

fn validate_member(member: &str) -> Result<()> {
    validate_member_name(member, "member name")?;
    Ok(())
}

#[cfg(test)]
mod tests;
