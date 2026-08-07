# tensor-dbus

`tensor-dbus` is TensorDE's Compio-native asynchronous D-Bus implementation for
Linux. It provides bus and peer clients, peer listeners and authentication,
typed object services, standard object interfaces, signal routing, bus-name
ownership, dynamic message bodies, and Unix file descriptor passing without
creating a runtime, worker thread, or blocking facade.

The caller owns the Compio runtime and the event loop. A `Connection` is a
single-owner object that performs I/O only when one of its async methods is
awaited.

```rust
use tensor_dbus::Connection;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let runtime = compio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let mut connection = Connection::session_bus().await?;
        let names: Vec<String> = connection
            .call(
                Some("org.freedesktop.DBus"),
                "/org/freedesktop/DBus",
                Some("org.freedesktop.DBus"),
                "ListNames",
                &(),
            )
            .await?;
        println!(
            "{} names on {}",
            names.len(),
            connection.unique_name().expect("bus connections have a unique name")
        );
        Ok::<_, tensor_dbus::Error>(())
    })?;
    Ok(())
}
```

## API model

- `Connection::call` is the direct typed request/reply path.
- `Connection::session_bus`, `system_bus`, and `connect_bus` create message-bus
  connections. `connect_peer` and `accept_peer` create direct peer-to-peer
  connections over caller-owned Compio Unix streams; peer method destinations
  and interfaces are optional.
- `Connection::send_call` returns a four-byte `PendingReply<R>` token, allowing
  several calls to remain in flight on one connection.
- `MethodCallFlags` and the `*_with_flags` methods expose no-autostart and
  interactive-authorization policy. `send_no_reply` is the explicit
  `NO_REPLY_EXPECTED` path and cannot accidentally return a pending token.
- `MethodCall` and the `reply_method*` helpers support services driven by an
  explicit `Connection::receive` loop.
- `ObjectServer` registers typed asynchronous handlers and properties, routes
  methods by object/interface/member, and supplies Introspectable, Peer,
  Properties, and ObjectManager behavior. Typed signal registration adds
  custom signal contracts to generated introspection XML without changing the
  emission path. Property registration requires an explicit
  `PropertyChangeMode`: `Value`, `Invalidates`, `Const`, or `Silent` generate
  the corresponding `EmitsChangedSignal` annotation. A successful writable
  property `Set` automatically emits the configured notification; `Value`
  mode reads the property back after the setter so normalized service state is
  emitted instead of echoing uncommitted input. `MethodContext` exposes the bus
  sender or kernel-authenticated P2P credentials to authorization-sensitive
  handlers. Handlers and futures need not be `Send`.
  `register_with_connection` accepts a native Rust async closure that can emit
  signals or perform nested calls on the active Compio connection before
  returning its method result.
- `PeerListener::accept` separates socket admission from authentication so a
  service can bound or spawn handshake futures without letting a slow client
  serialize the accept loop. `accept_authenticated` is the single-connection
  convenience path.
- `Message::body_dynamic` decodes an unknown runtime signature into an owned,
  re-encodable `DynamicBody` while retaining Unix descriptor ownership.
  `DynamicBody::from_fields` builds calls, replies, or signals from owned
  runtime-typed top-level arguments.
- `MatchRule`, `Connection::add_match`, and `Connection::remove_match` support
  caller-owned multi-rule signal loops. Rules cover exact and namespace paths,
  unique destinations, string arguments, object-path arguments, and argument
  zero bus-name namespaces with the D-Bus length and index limits enforced.
  `sender_available` exposes whether a well-known sender currently has an
  owner, so products can implement owner-driven recovery without polling.
  `SignalStream` remains the convenient exclusive-borrow path for one rule.
- `zvariant` is re-exported for D-Bus ABI types and body encoding, including
  Unix file descriptors.
- `freedesktop::upower` provides a reusable typed display-device monitor. It
  installs root and device matches before the initial read, pipelines the two
  `GetAll` calls, validates property types atomically, and reports owner
  changes or invalidated properties as explicit refresh boundaries. Products
  retain and lower the resulting snapshot; the adapter owns no UI or power
  policy.
- `freedesktop::login1` resolves a process-owned logind session, installs one
  exact `Lock`/`Unlock` signal rule before reading `LockedHint`, and exposes
  explicit `SetLockedHint`, lock-all, and suspend calls without owning a task
  or reconnect policy.
- `freedesktop::mpris` discovers a bounded set of session players, installs
  owner, property, and `Seeked` matches before its initial reads, validates
  metadata, position, duration, and capabilities into retained snapshots, and
  selects a stable active player. `Previous`, `PlayPause`, and `Next` reject
  unsupported capabilities before issuing a call. The caller owns the Compio
  receive loop, action queue, refresh boundary, and product policy.
- `freedesktop::network_manager` retains NetworkManager's typed aggregate
  state, connectivity, primary-connection kind, and Wi-Fi radio state. It
  installs the root property match before `GetAll`, applies property changes
  atomically, exposes invalidation and owner changes as explicit refresh
  boundaries, and provides standard overall-networking and Wi-Fi write calls.
  Its `wifi` child module adds one owner-aware namespace monitor and a bounded,
  completion-pipelined inventory of Wi-Fi devices and access points. Raw SSID
  bytes are retained alongside bounded display text; device state, active AP,
  signal, frequency, bitrate, mode, WPA/RSN flags, secured and enterprise
  classification are typed. `GetAllAccessPoints` is the only discovery API,
  and `request_scan` submits an empty `a{sv}` options map. Topology and scan
  completion are explicit refresh boundaries while AP strength and active-AP
  changes apply atomically. It owns no credentials, saved-connection policy,
  reconnect policy, or UI policy.
- `tensor::shell` defines the versioned Tensor Shell media-control destination,
  object, interface, stable method mapping, and Compio client call. Tensor Shell
  owns the service object and action policy; `tensor-dbus` does not start or
  discover the product.

Signal headers contain unique sender names. Installing a rule for a well-known
sender resolves its current unique owner and installs a bounded
`NameOwnerChanged` match for that exact name. `SignalStream` maintains this
mapping automatically. Caller-owned multi-rule loops call `MatchRule::observe`
for each installed rule before `MatchRule::matches`, preserving correct routing
across owner handoffs without a hidden task.

## Peer service

The listener, task admission policy, and connection lifetime remain explicit:

```rust
use tensor_dbus::{Guid, MethodError, ObjectServer, PeerListener};

# async fn serve() -> tensor_dbus::Result<()> {
let listener = PeerListener::bind("/run/user/1000/example-dbus", Guid::generate()?).await?;
let accepted = listener.accept().await?;
let credentials = accepted.credentials();
// Apply UID/PID admission and concurrency policy before authentication.
let mut connection = accepted.authenticate().await?;

let mut objects = ObjectServer::new();
objects.register::<String, String, _, _>(
    "/org/example/Service",
    "org.example.Service",
    "Echo",
    |value| async move { Ok::<_, MethodError>(value) },
)?;
loop {
    if let Some(message) = objects.serve_next(&mut connection).await? {
        // The same connection may also carry signals or replies.
        drop(message);
    }
}
# }
```

## Cancellation

Dropping an async wait does not cancel the method running in the remote
process. For a caller-owned timeout or select operation:

1. Submit with `send_call` and retain the `PendingReply`.
2. Race `PendingReply::wait_message` against the caller's timer.
3. Decode the returned message with `PendingReply::decode`, or consume the
   token with `PendingReply::abandon` after a timeout.

Abandonment is local routing state. A late reply is discarded the next time
the caller drives the connection.

Socket reads, frame bytes, ancillary file descriptors, and socket writes are
owned by the `Connection`, not by the outer wait future. Cancelling
`wait_message` in the middle of a frame pauses that exact Compio operation; the
next receive or wait resumes it without losing bytes or descriptors. A
cancelled send can be completed explicitly with `Connection::flush`; the
message may already have reached the peer, so cancellation never means
"definitely not sent". Reply-producing sends should be awaited until they
return their `PendingReply`, otherwise the reply token itself is unavailable.

`Connection::close` completes a retained write, cancels a retained read, and
asynchronously closes the socket. Dropping the connection is the immediate
abort path and does not promise to finish an interrupted write.

## Protocol boundaries

- Unix `path` and Linux abstract sockets are supported for bus connections and
  P2P listeners. Ordered bus address lists are tried until one endpoint
  completes authentication and the bus `Hello`.
- An address-provided `guid` is validated against the authenticated server
  GUID; `Connection::server_guid` exposes the verified identity.
- EXTERNAL authentication and Unix FD negotiation are supported.
- P2P accept validates the EXTERNAL identity against Linux `SO_PEERCRED`; the
  caller owns the `UnixListener`, accept loop, connection concurrency, and
  authorization policy above the authenticated kernel credentials.
- Incoming little-endian and big-endian messages are decoded; outgoing
  messages use little endian.
- Messages are limited to 16 MiB and 64 Unix file descriptors.
- Received descriptors use Linux `MSG_CMSG_CLOEXEC`, so close-on-exec is set
  atomically by `recvmsg` rather than in a post-receive `fcntl` window.
- The pending-message queue is limited to 256 entries and 32 MiB of retained
  wire data; the abandoned-reply registry is limited to 256 entries.
- Header fields, names, signatures, body types, serials, ancillary data, and
  exact body consumption are validated at their protocol boundaries.
- Incoming bodies are decoded directly from the retained frame allocation;
  routing does not copy the body into a second buffer. Validated header strings
  are range views into that same allocation rather than per-field `String`s.
- Outgoing headers and bodies are built in one final frame buffer. Common small
  messages avoid a temporary header allocation and header copy.
- Incoming reads reserve 512 bytes for common control and desktop-service
  messages, avoiding the fixed-header-to-frame reallocation without attaching
  a 4 KiB allocation to every small frame. Larger frames grow once to their
  validated total length while retaining the 16 MiB cap.
- `Message` debug output contains bounded metadata, never the retained body or
  file-descriptor contents.

Capacity failures are explicit errors. The crate does not silently discard
unrelated messages or weaken FD negotiation.

## Performance baseline

Run the release-mode private-bus harness to measure both sequential round-trip
cost and bounded in-flight throughput on the current machine:

```bash
cargo run -p tensor-dbus --release --example private_bus_benchmark -- 10000 32 64 1000
```

The arguments are throughput operations, payload bytes, batch size, and
sequential operations. The harness warms the connection first, verifies every
echoed payload, and reports elapsed time, operations per second, and average
microseconds per operation. A process-local counting allocator also reports
allocation calls and requested bytes for the combined caller-driven client and
service path; the separately spawned `dbus-daemon` is outside those counts. The
harness intentionally has no time or allocation assertion, so the output is a
comparable measurement rather than a scheduler-sensitive test.

Run the peer object-server harness with the same arguments to isolate direct
Unix transport plus typed routing from message-bus broker overhead:

```bash
cargo run -p tensor-dbus --release --example peer_object_server_benchmark -- 10000 32 64 1000
```

On the current development machine with a 32-byte body and batch size 64, the
object-server path measured about 165k operations/second, 6.04 us/operation,
and 39.015 allocations/operation. Explicit-interface dispatch borrows the
validated header and performs no target-name allocation; the erased async
handler future is the one framework-specific per-call allocation.

The connection retains one boxed read or write state machine while an operation
is in flight. This fixed allocation is what lets a caller drop an outer future
and later resume the exact Compio operation with its stream buffer and Unix file
descriptors intact. Replacing it requires an explicit poll state machine and
must preserve the cancellation tests before benchmark numbers are compared.

## Non-goals

The crate does not provide a private executor, hidden cleanup task, synchronous
wrapper, remote method cancellation, TCP or nonce-TCP transport, autolaunch
fallback, or a D-Bus message-bus broker. TensorDE products own service policy,
retry policy, timeout duration, listener concurrency, task structure, and
runtime lifetime.
