# Tensor product naming

TensorDE uses one product family rather than unrelated application brands. The
public application names follow the same model as the Hyprland ecosystem: the
compositor carries the distinctive family name and companion applications use
the `Tensor` prefix plus their role.

| Product role | Product name | Cargo package / executable | Repository path |
| --- | --- | --- | --- |
| Wayland compositor | Tensorland | `tensorland` / `tensor` | `apps/tensor-wm` |
| Desktop shell | Tensor Shell | `tensor-shell` | `apps/tensor-shell` |
| Application launcher | Tensor Launcher | `tensor-launcher` | `apps/tensor-launcher` |
| Login greeter | Tensor Greeter | `tensor-greeter` | `apps/tensor-greeter` |
| Settings application | Tensor Settings | `tensor-settings` | `apps/tensor-settings` |
| Idle policy service | Tensor Idle | `tensor-idle` | `apps/tensor-idle` |
| Product IPC client | Tensor Msg | `tensor-msg` | `apps/tensor-msg` |
| File manager | Tensor Files | `tensor-files` | `apps/tensor-files` |
| Scene wallpaper engine | Tensor Wallpaper | `tensor-wallpaper` | `apps/tensor-wallpaper` |
| Desktop portal backend | Tensor XDP | `tensor-xdp` | `apps/tensor-xdp` |

The compositor command is `tensor`; the independently packaged `tensor-msg`
client uses `tensor-msg land` for compositor IPC. Its companion
commands are `tensor-session` and `tensor-dmabuf-smoke`. The wallpaper product made a coordinated hard cutover from
Gilder to Tensor Wallpaper. Its only active commands are `tensor-wallpaper`,
`tensor-wallpaper-convert` and `tensor-wallpaperd`; its control commands are
provided by `tensor-msg wallpaper`, and its only active
application path is `apps/tensor-wallpaper`.

`tensor-msg` is the standalone family control frontend. Tensorland operations use the `land`
subcommands, following `niri msg`; Wallpaper uses `wallpaper`, and any future
Shell operations use an explicit product subcommand without moving policy into
a shared daemon.
Launcher, greeter, settings, and idle do not publish family IPC endpoints.

Tensor XDP owns `org.freedesktop.impl.portal.desktop.tensor` and the
`tensor.portal` backend descriptor. Tensor Files does not retain a private XDP
FileChooser service.

## Stable family interfaces

The rename does not change shared `tensor-*` crate names, KDL vocabulary,
the compositor's `TENSOR_*` environment variables, the `tensor.sock` IPC default, or durable
correctness/evidence roots under `references/fika`, `references/tensor`, and
`artifacts/tensor`. Those names identify the TensorDE family or retained source
evidence rather than a public application executable.

System integration uses the new compositor identity directly:

- `tensorland.desktop`, `DesktopNames=Tensorland`, and `Exec=tensor-session`;
- `tensorland.service` and `tensorland-shutdown.target`;
- `tensor --session` as the compositor service command.

Tensor Files uses `TENSOR_FILES_*` for its product-specific runtime controls,
`org.tensorde.TensorFiles1.Privileged` for its privileged D-Bus API, and
`org.tensorde.TensorFiles` for its Wayland app-id.

TensorDE is pre-release, so source paths, Cargo package names, executable names,
and system integration make a coordinated hard cutover. Do not add duplicate
old binaries, packages, service units, or hidden runtime fallbacks. A diagnostic
may mention the replacement command when rejecting an obsolete invocation or
configuration, but there is only one active implementation.

## Product-family audit

The app boundary is informed by the official
[COSMIC](https://github.com/pop-os) and [Hyprland](https://github.com/hyprwm)
families, reviewed on 2026-08-05. COSMIC deliberately uses a fine-grained set
of `cosmic-comp`, `cosmic-session`, `cosmic-panel`, `cosmic-applets`,
`cosmic-settings`, `cosmic-settings-daemon`, `cosmic-launcher`, `cosmic-bg`,
`cosmic-notifications`, `cosmic-osd`, `xdg-desktop-portal-cosmic`,
`cosmic-greeter`, and `cosmic-randr`. Hyprland's official family separates the
compositor, `hyprpaper`, `hypridle`, `hyprlock`,
`xdg-desktop-portal-hyprland`, `hyprlauncher`, and `hyprpolkitagent`, while
keeping monitor control in compositor IPC/clients rather than requiring a
desktop monitor service.

Tensor uses process separation only for a lifecycle, packaging, security, or
failure-isolation boundary:

| Reference responsibility | Tensor owner | Decision |
| --- | --- | --- |
| compositor + session bootstrap | `tensor` + `tensor-session` in `apps/tensor-wm` | Keep one product; session bootstrap is a small companion executable, not another UI app. |
| panel, applets, notification center, OSD, overview, lock surface | `tensor-shell` | Keep one retained Shell process because these surfaces share compositor state, rendering, and visibility policy. |
| launcher + app library/catalog | `tensor-launcher` | One short-lived app; do not create a second app-library product. |
| wallpaper/background service | `tensor-wallpaper` | Keep independently installable so Wallpaper does not require the compositor package. |
| idle and output-power policy | `tensor-idle` | Keep independent from Shell; it owns idle deadlines and direct protocol actions. |
| settings UI | `tensor-settings` | Keep independent, but do not add a generic settings daemon while product owners can apply their own typed configuration. |
| portal backend | `tensor-xdp` | Keep independent for D-Bus activation and the portal security boundary. |
| login greeter | `tensor-greeter` | Keep independent because it runs before the user session and talks directly to greetd. |
| family IPC CLI | `tensor-msg` | Keep a small independently packaged client; it is not a daemon. |
| file manager | `tensor-files` | Optional end-user product, not required to start a Tensor session. |

The resulting installation tiers are intentionally smaller than the union of
the two reference families:

| Installation tier | Apps | Contract |
| --- | --- | --- |
| Minimal compositor session | `tensor` + `tensor-session` | Hard runtime core. A terminal or configured binding can start applications without any other Tensor app. |
| Default Tensor desktop | `tensor-shell`, `tensor-launcher`, `tensor-idle` | Shipped and enabled by default for complete desktop, lock, launch, idle, and DPMS behavior, but independently replaceable or disableable. |
| Activated desktop integration | `tensor-xdp` | D-Bus activated when portal operations are requested; not a permanently required session process. |
| Configuration and administration | `tensor-settings`, `tensor-msg` | On-demand UI/CLI. The compositor continues from typed KDL without either process. |
| Login and visual products | `tensor-greeter`, `tensor-wallpaper`, `tensor-files` | Installed only when that login path or user-facing product is wanted. |

Thus only Tensor WM and its session launcher are unconditional. Shell,
Launcher, and Idle define the recommended Tensor desktop experience, while all
other products are capability-driven or on-demand rather than boot-time
dependencies.

There is no `tensor-monitor` or `tensor-randr` app. Output discovery,
configuration, layout, color state, and DPMS remain distinct operations:

- Tensor WM owns output topology and applies atomic output configuration;
- Tensor Settings owns the display configuration page;
- `tensor-msg land` provides scriptable output operations;
- Tensor Idle owns DPMS transitions without disabling outputs or changing the
  desktop topology.

Likewise, notification, OSD, lock, screenshot, blue-light, system-information,
audio-control, theme-editor, and shutdown frontends do not become separate apps
merely because a reference family split them. They remain Shell, Settings,
portal, compositor, or session features. A PolicyKit authentication agent is
the one plausible future security-isolated app; it should be added only after
`tensor-dbus` and the secure prompt lifecycle are vertically complete, with
the repository contract updated in the same slice.
