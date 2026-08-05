use std::{os::fd::AsFd, path::Path};

use compio::net::{UnixListener, UnixStream};

use crate::{Connection, Error, Guid, PeerCredentials, Result};

/// A Compio Unix listener carrying one stable D-Bus server GUID.
///
/// [`Self::accept`] deliberately stops before authentication. This lets an
/// accept loop inspect kernel credentials, enforce admission limits, and spawn
/// each [`AcceptedPeer::authenticate`] future independently so a stalled
/// handshake cannot serialize later accepts.
pub struct PeerListener {
    listener: UnixListener,
    server_guid: Guid,
}

impl PeerListener {
    pub async fn bind(path: impl AsRef<Path>, server_guid: Guid) -> Result<Self> {
        Ok(Self {
            listener: UnixListener::bind(path).await?,
            server_guid,
        })
    }

    pub const fn from_listener(listener: UnixListener, server_guid: Guid) -> Self {
        Self {
            listener,
            server_guid,
        }
    }

    pub const fn server_guid(&self) -> Guid {
        self.server_guid
    }

    pub async fn accept(&self) -> Result<AcceptedPeer> {
        let (stream, _) = self.listener.accept().await?;
        let credentials = peer_credentials(&stream)?;
        Ok(AcceptedPeer {
            stream,
            server_guid: self.server_guid,
            credentials,
        })
    }

    /// Accepts and authenticates one connection inline.
    ///
    /// Services accepting multiple clients should generally use [`Self::accept`]
    /// and drive each returned authentication future concurrently.
    pub async fn accept_authenticated(&self) -> Result<Connection> {
        self.accept().await?.authenticate().await
    }

    pub fn into_inner(self) -> UnixListener {
        self.listener
    }
}

/// An accepted Unix peer whose D-Bus authentication has not started.
pub struct AcceptedPeer {
    stream: UnixStream,
    server_guid: Guid,
    credentials: PeerCredentials,
}

impl AcceptedPeer {
    pub const fn credentials(&self) -> PeerCredentials {
        self.credentials
    }

    pub const fn server_guid(&self) -> Guid {
        self.server_guid
    }

    pub async fn authenticate(self) -> Result<Connection> {
        Connection::accept_peer_with_credentials(self.stream, self.server_guid, self.credentials)
            .await
    }

    pub fn into_stream(self) -> UnixStream {
        self.stream
    }
}

fn peer_credentials(stream: &UnixStream) -> Result<PeerCredentials> {
    let credentials = rustix::net::sockopt::socket_peercred(stream.as_fd())
        .map_err(|error| Error::Io(error.into()))?;
    Ok(PeerCredentials {
        process_id: credentials.pid.as_raw_pid() as u32,
        user_id: credentials.uid.as_raw(),
        group_id: credentials.gid.as_raw(),
    })
}
