# Tensor Shell Functional Alignment

Tensor Shell aligns behavior against the pinned local references, not against
screenshots or a generic layer-shell demo:

- DankMaterialShell `6de5593216548551db507cecde581558475972a6`
- StatIndet/quickshell `c94c62ad7131dbd2bd162c9c9adef6076c6c6e47`

The references are correctness and interaction sources only. Tensor Shell is a
native Rust/Vulkan product and does not import QML, Qt, Go backends, reference
assets, or their runtime dependency choices.

## Alignment Matrix

| Area | Required behavior | Status |
| --- | --- | --- |
| Multi-output shell | Per-output lifecycle, reconnect recovery, explicit focus and modal ownership | Surface model and shared Vulkan lifecycle implemented |
| Panel | Workspaces, launcher entry, active window, tray, media, system status, clock/calendar and configurable ordering | KDL ordering, responsive retained widget geometry, Vulkan draw list, hit testing, pointer/touch entry actions, fixed retained applet state, live notification badge/attention, and typed signal-driven UPower status implemented; text/icons, remaining live services and tray pending |
| Launcher | Applications, files, windows, calculator and commands; Spotlight modes for apps, wallpaper, clipboard and web | Standalone product has KDL, cold desktop-entry discovery and bounded retained application search; native surface, input and non-app providers pending; Shell placeholder awaits removal |
| Notifications | Freedesktop service, replacement, bounded popup queue, grouping, history, DND, actions and keyboard navigation | Service and value model in progress; UI/persistence pending |
| OSD | Audio output, volume/mute, microphone, brightness, media, power profile, caps lock and idle inhibit | Visible chrome surface only; service state pending |
| Control center | Network, Bluetooth, audio devices, display/night mode, battery/power, theme and session actions | Visible chrome surface only; controls and services pending |
| Overview | Tensorland workspaces/windows, activation, move and close operations, per-output presentation | Visible chrome surface only; Tensorland integration pending |
| Lock/session | `ext-session-lock`, PAM authentication and lock-safe notifications; independently requested by Tensor Idle | Surface placeholder only; idle policy extracted to `tensor-idle` |
| Media and clipboard | MPRIS controls and metadata; MIME-aware clipboard history with bounded image previews | Not started |
| System monitoring | CPU, memory, GPU, disk, network, battery and bounded process inspection | Not started |
| Theme and settings | Material token model, light/dark and wallpaper-derived colors, motion/typography and persisted settings | Typed Shell KDL and panel ordering implemented; settings UI and product schema/reload work extracted to `tensor-settings` |
| Extensibility | Stable typed widget/service contracts; external plugins only after permission and lifecycle policy exists | Not started |

## Delivery Order

1. Complete panel text/icon pipelines, Tensorland workspace/window snapshots,
   tray/media/status services, and runtime KDL reload on the implemented
   configurable geometry, hit testing, pointer/touch actions, and shared
   multi-output Vulkan presentation lifecycle.
2. Complete notifications end to end: value model, freedesktop service,
   popup/center scene building, actions and persistence.
3. Connect Tensor Shell's launcher entry to the standalone Tensor Launcher,
   add its native surface/input and activation-aware Tensorland IPC, then add
   file, window, calculator and command providers without blocking the UI thread.
4. Reuse the implemented typed UPower snapshot in Tensor Idle and the
   control-center power surface; connect NetworkManager, BlueZ,
   PipeWire/WirePlumber and MPRIS through equivalent retained adapters and
   share their validated snapshots with OSD and controls. Preserve explicit
   pending, unavailable, failed and attention states.
5. Implement overview and `ext-session-lock` as protocol-correct workflows;
   layer-shell lock placeholders are not a completed lock screen.
6. Add media, clipboard, monitoring, theme/settings and vetted extensibility.

Cold discovery, parsing, image decoding, search indexing and shader preparation
stay outside frame work. Runtime services use bounded queues and retained
snapshots. Every surface uses `vulkan-renderer`; feature delivery must not add a
QML, wgpu, CPU-rendering, or legacy descriptor path.
