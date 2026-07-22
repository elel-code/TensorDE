# IPC and Portal Gates

The compositor control protocol uses a Unix socket, a little-endian `u32` frame length, and a JSON
envelope. Every request carries a protocol version and request ID. Frames are capped at 1 MiB, the
socket is mode `0600`, bind never removes an existing path, and drop removes only the inode owned by
that server instance.

Connection handling must verify peer credentials before dispatch. Commands enter a bounded queue
and are validated before becoming ECS events. External clients never receive Smithay objects,
Vulkan handles, or direct mutable ECS access.

XDP means xdg-desktop-portal in this repository. The future portal implementation is an optional
D-Bus/PipeWire adapter for controlled capture and sharing. It follows the same command gate as IPC
and cannot bypass renderer extraction policy.

systemd is separately optional. Readiness is an output of the completed startup state, not a
prerequisite or an owner of the compositor lifecycle.
