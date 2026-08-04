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
| Panel | Workspaces, launcher entry, active window, tray, media, system status, clock/calendar and configurable ordering | Retained Vulkan presentation implemented; widget scene and input pending |
| Launcher | Applications, files, windows, calculator and commands; Spotlight modes for apps, wallpaper, clipboard and web | Visible chrome surface only; providers and input pending |
| Notifications | Freedesktop service, replacement, bounded popup queue, grouping, history, DND, actions and keyboard navigation | Service and value model in progress; UI/persistence pending |
| OSD | Audio output, volume/mute, microphone, brightness, media, power profile, caps lock and idle inhibit | Visible chrome surface only; service state pending |
| Control center | Network, Bluetooth, audio devices, display/night mode, battery/power, theme and session actions | Visible chrome surface only; controls and services pending |
| Overview | Tensorland workspaces/windows, activation, move and close operations, per-output presentation | Visible chrome surface only; Tensorland integration pending |
| Lock/session | `ext-session-lock`, PAM authentication, idle-driven lock/suspend, DPMS transitions and lock-safe notifications | Surface placeholder only |
| Media and clipboard | MPRIS controls and metadata; MIME-aware clipboard history with bounded image previews | Not started |
| System monitoring | CPU, memory, GPU, disk, network, battery and bounded process inspection | Not started |
| Theme and settings | Material token model, light/dark and wallpaper-derived colors, motion/typography and persisted settings | Not started |
| Extensibility | Stable typed widget/service contracts; external plugins only after permission and lifecycle policy exists | Not started |

## Delivery Order

1. Build the real retained panel scene, hit testing, and input on the shared
   multi-output Vulkan presentation lifecycle.
2. Complete notifications end to end: value model, freedesktop service,
   popup/center scene building, actions and persistence.
3. Deliver Spotlight application search and Tensorland workspace/window data,
   then add the remaining providers without blocking the UI thread.
4. Add service-backed panel widgets, OSD and control-center controls with
   explicit unavailable/error states.
5. Implement overview and `ext-session-lock` as protocol-correct workflows;
   layer-shell lock placeholders are not a completed lock screen.
6. Add media, clipboard, monitoring, theme/settings and vetted extensibility.

Cold discovery, parsing, image decoding, search indexing and shader preparation
stay outside frame work. Runtime services use bounded queues and retained
snapshots. Every surface uses `vulkan-renderer`; feature delivery must not add a
QML, wgpu, CPU-rendering, or legacy descriptor path.
