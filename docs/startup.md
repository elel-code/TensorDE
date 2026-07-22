# Startup

The startup sequence is a set of ordered gates:

1. Parse CLI arguments and environment overrides.
2. Resolve, parse, and validate the complete KDL configuration.
3. Initialize logging and diagnostics.
4. Create calloop, the Smithay display, and the Wayland listening socket.
5. Create Vulkan and reject devices without `VK_EXT_descriptor_heap`.
6. Bind the private IPC socket.
7. Construct Bevy ECS resources, schedules, and the initial scene.
8. Register signals, configuration watchers, IPC, input, and backend event sources.
9. Enter the event loop.
10. Notify optional systemd integration only after every required gate succeeds.

`--check` executes the initialization gates that exist, reports their resolved state, and exits. It
must never emit systemd readiness. Partial initialization must unwind owned sockets and handles.

Niri is the primary lifecycle reference, particularly its ordering of configuration, calloop,
Wayland display/socket, IPC, environment publication, watcher registration, and final run loop.
