# IPC and Portal Gates

The compositor control protocol uses a Unix socket, a little-endian `u32` frame length, and a JSON
envelope. Every request carries a protocol version and request ID. Frames are capped at 1 MiB, the
socket is mode `0600`, bind never removes an existing path, and drop removes only the inode owned by
that server instance.

The server is registered in the Smithay calloop. Each client has bounded frame decoding, a bounded
response queue, and a separate non-blocking response writer, so a slow IPC peer cannot stall
Wayland dispatch or grow compositor memory without limit.
`tensor-msg` exposes `ping`, `get-state`, `set-layout`, `spawn`, and `quit` using the same codec.

`spawn` accepts a direct argv array (one program and optional arguments). The compositor queues the
command on the asynchronous launch worker and returns `accepted` as soon as the request is enqueued;
process creation and optional systemd scope setup complete off the calloop thread. Empty argv is
rejected with `invalid_argument`. A saturated launch queue returns `queue_full`. Tensor never
invokes a shell for this path.

Connection handling verifies peer credentials before dispatch. Requests are validated and dispatched
on the compositor event-loop thread; they do not receive direct access to ECS, Smithay, or Vulkan
objects. External clients never receive Smithay objects, Vulkan handles, or mutable ECS access.

XDP means xdg-desktop-portal in this repository. The future portal implementation is an optional
D-Bus/PipeWire adapter for controlled capture and sharing. It follows the same command gate as IPC
and cannot bypass renderer extraction policy.

systemd is separately optional. Readiness is an output of the completed startup state, not a
prerequisite or an owner of the compositor lifecycle.
