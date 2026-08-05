# Tensor control plane

TensorDE keeps runtime policy in the product that owns it. Tensorland's IPC
server, launch/session startup, Wayland objects, ECS, and renderer gates remain
under `apps/tensor-wm`; Tensor Shell and Tensor Wallpaper likewise retain their
own command semantics and lifecycle. A shared crate must not become a desktop
daemon or depend on one of these applications.

## Standalone unified client

The user-facing frontend is a separate lightweight executable:

```text
tensor-msg land ...
tensor-msg wallpaper ...
```

`tensor` is only the compositor process under `apps/tensor-wm`. `tensor-msg`
lives under `apps/tensor-msg`, depends on the value-only `tensor-ipc` crate,
and does not link the compositor, Wallpaper scene engine, Vulkan renderer, or
Wayland runtime. It can therefore be packaged with Wallpaper without pulling
in Tensor WM. Each product subcommand discovers that product's versioned
endpoint, performs bounded request/reply or event-stream I/O, and renders
structured errors. Routing does not proxy messages through the compositor and
does not grant one product ownership of another product's state.

`tensorctl` and `tensor-wallpaperctl` are removed after their operations are
covered by `tensor-msg land` and `tensor-msg wallpaper`; compatibility aliases
are not accumulated in this pre-release workspace. A future `tensor-msg shell`
target is added only when Tensor Shell exposes a real bounded service. Tensor
Launcher, Tensor Greeter, Tensor Settings, and Tensor Idle expose no family IPC
server and therefore have no `tensor-msg` product target.

## Shared and product-local boundaries

- Framing, request IDs, completion-oriented Unix transport, endpoint metadata,
  and bounded client buffers may become reusable crates.
- Command enums, authorization, session launch policy, and server state remain
  inside the owning app.
- Credentials never traverse the family control plane. Tensor Greeter speaks
  the greetd protocol directly, and greetd retains PAM/session authority.
- Renderer, Wayland, DRM/KMS, ECS, and input handles never cross IPC. Snapshots
  and commands are value-only.
- Tensor Launcher is a short-lived ordinary Wayland client. It obtains an
  `xdg-activation-v1` token and performs a direct argv launch, optionally inside
  a configured systemd user scope; it does not expose or require product IPC.
- Tensor Idle owns desktop idle policy and subscribes directly to
  `ext-idle-notify-v1`. It coordinates protocol-correct lock, DPMS, and logind
  suspend boundaries without making Tensor Shell an idle daemon. Tensor WM
  remains authoritative for activity timing and
  `zwp_idle_inhibit_manager_v1`; neither Shell nor Idle emulates them through
  private IPC.
- Tensor Settings is a short-lived ordinary Wayland application. It edits each
  product's own typed configuration and invokes that product's reload route;
  it is not a central settings daemon.

All product clients use the repository's completion model. There is no polling
or readiness fallback hidden behind the unified command.
