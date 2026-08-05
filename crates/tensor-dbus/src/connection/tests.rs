use compio::io::AsyncRead;
use tensor_runtime::io_uring_runtime;

use super::*;
use crate::wire::{Outgoing, decode_message, encode_outgoing};

fn test_message() -> Message {
    let encoded = encode_outgoing(
        Outgoing {
            kind: MessageKind::Signal,
            flags: 0,
            serial: 1,
            reply_serial: None,
            path: Some("/org/tensor/Test"),
            interface: Some("org.tensor.Test"),
            member: Some("Changed"),
            error_name: None,
            destination: None,
        },
        &(),
    )
    .unwrap();
    decode_message(encoded.bytes, Vec::new()).unwrap()
}

async fn test_connection() -> Connection {
    let (client, _server) = std::os::unix::net::UnixStream::pair().unwrap();
    let stream = UnixStream::from_std(client).unwrap();
    Connection {
        reader: MessageReader::new(stream.clone(), false, Vec::new(), Vec::new()),
        writer: MessageWriter::new(stream, false),
        next_serial: 1,
        pending: VecDeque::new(),
        pending_bytes: 0,
        abandoned_replies: AbandonedReplies::new(),
        routing_failed: false,
        mode: ConnectionMode::Bus,
        peer_credentials: None,
        server_guid: "0123456789abcdef0123456789abcdef".parse().unwrap(),
        unique_name: Some(":1.1".to_owned()),
        unix_fd: false,
    }
}

#[test]
fn abandoned_reply_registry_is_bounded_and_reusable() {
    let mut replies = AbandonedReplies::new();
    for serial in 1..=MAX_ABANDONED_REPLIES as u32 {
        replies.register(serial).unwrap();
    }
    assert!(matches!(
        replies.register(MAX_ABANDONED_REPLIES as u32 + 1),
        Err(Error::AbandonedReplyQueueFull {
            limit: MAX_ABANDONED_REPLIES
        })
    ));
    assert!(replies.take(1));
    replies.register(MAX_ABANDONED_REPLIES as u32 + 1).unwrap();
}

#[test]
fn abandoned_reply_overflow_is_a_terminal_routing_failure() {
    io_uring_runtime(2).unwrap().block_on(async {
        let mut connection = test_connection().await;
        for serial in 1..=MAX_ABANDONED_REPLIES as u32 {
            connection
                .abandon_reply(PendingReply::<()>::new(serial))
                .unwrap();
        }

        assert!(matches!(
            connection.abandon_reply(PendingReply::<()>::new(MAX_ABANDONED_REPLIES as u32 + 1)),
            Err(Error::AbandonedReplyQueueFull {
                limit: MAX_ABANDONED_REPLIES
            })
        ));
        assert!(!connection.is_usable());
        assert!(matches!(
            connection.flush().await,
            Err(Error::ConnectionUnusable)
        ));
        assert!(matches!(
            connection.abandon_reply(PendingReply::<()>::new(MAX_ABANDONED_REPLIES as u32 + 2)),
            Err(Error::ConnectionUnusable)
        ));
    });
}

#[test]
fn pending_count_overflow_is_a_terminal_routing_failure() {
    io_uring_runtime(2).unwrap().block_on(async {
        let mut connection = test_connection().await;
        let message = test_message();
        connection.pending_bytes = message.wire_len() * MAX_PENDING_MESSAGES;
        connection.pending = std::iter::repeat_n(message.clone(), MAX_PENDING_MESSAGES).collect();

        assert!(matches!(
            connection.queue_pending(message),
            Err(Error::PendingQueueFull {
                limit: MAX_PENDING_MESSAGES
            })
        ));
        assert!(!connection.is_usable());
        assert!(matches!(
            connection.receive().await,
            Err(Error::ConnectionUnusable)
        ));
    });
}

#[test]
fn pending_byte_overflow_is_a_terminal_routing_failure() {
    io_uring_runtime(2).unwrap().block_on(async {
        let mut connection = test_connection().await;
        connection.pending_bytes = MAX_PENDING_BYTES;

        assert!(matches!(
            connection.queue_pending(test_message()),
            Err(Error::PendingBytesFull {
                limit: MAX_PENDING_BYTES
            })
        ));
        assert!(!connection.is_usable());
        assert!(matches!(
            connection.flush().await,
            Err(Error::ConnectionUnusable)
        ));
    });
}

#[test]
fn explicit_close_cancels_a_retained_read_and_closes_the_socket() {
    io_uring_runtime(2).unwrap().block_on(async {
        let (client, server) = std::os::unix::net::UnixStream::pair().unwrap();
        let stream = UnixStream::from_std(client).unwrap();
        let mut peer = UnixStream::from_std(server).unwrap();
        let mut connection = Connection {
            reader: MessageReader::new(stream.clone(), false, Vec::new(), Vec::new()),
            writer: MessageWriter::new(stream, false),
            next_serial: 1,
            pending: VecDeque::new(),
            pending_bytes: 0,
            abandoned_replies: AbandonedReplies::new(),
            routing_failed: false,
            mode: ConnectionMode::Bus,
            peer_credentials: None,
            server_guid: "0123456789abcdef0123456789abcdef".parse().unwrap(),
            unique_name: Some(":1.1".to_owned()),
            unix_fd: false,
        };

        let timed = compio::runtime::time::timeout(
            std::time::Duration::from_millis(10),
            connection.receive(),
        )
        .await;
        assert!(timed.is_err());
        connection.close().await.unwrap();

        let compio::BufResult(result, _) = peer.read(Vec::with_capacity(1)).await;
        assert_eq!(result.unwrap(), 0);
    });
}
