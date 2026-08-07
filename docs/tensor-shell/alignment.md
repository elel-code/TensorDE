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
| Panel | Workspaces, launcher entry, active window, tray, media, system status, clock/calendar and configurable ordering | Hot-reloadable KDL ordering, responsive retained widget geometry, Vulkan draw list, hit testing, pointer/touch entry actions, fixed retained applet state, live notification badge/attention, typed signal-driven UPower and MPRIS status, Media-to-Control-Center and Workspaces-to-Overview entries implemented; text/icons, remaining live services and tray pending |
| Launcher | Applications, files, windows, calculator and commands; Spotlight modes for apps, wallpaper, clipboard and web | Standalone product has KDL, cold desktop-entry discovery, bounded retained application search, ordinary Wayland/Vulkan surface, keyboard/pointer/text-input interaction and activation-aware Tensorland launch submission; Shell panel invokes its configured argv through bounded Compio `Spawn`; non-app providers and shared text/icon atlas rendering pending |
| Notifications | Freedesktop service, replacement, bounded popup queue, grouping, history, DND, actions and keyboard navigation | Freedesktop service, replacement/timeout semantics, bounded popup queue, grouping/history/DND state, retained center/popup cards, pointer and keyboard focus/action/dismiss routing, and `NotificationClosed`/`ActionInvoked` signals implemented; persistence and text/icon rendering pending |
| OSD | Audio output, volume/mute, microphone, brightness, media, power profile, caps lock and idle inhibit | Signal-driven media playback OSD, multi-output presentation, retained transport controls and seek/properties-driven progress, configurable timeout and hover pause implemented; text/art rendering and non-media services pending |
| Control center | Network, Bluetooth, audio devices, display/night mode, battery/power, theme and session actions | Retained Network root and bounded Wi-Fi device/AP discovery with SSID, signal, security and scan-refresh state; Lock, Suspend, DND and capability-aware Previous/PlayPause/Next controls, pointer and keyboard focus/navigation/activation, typed NetworkManager/UPower/MPRIS status, and explicit pending/succeeded/failed Compio Wi-Fi/logind/media actions implemented; credential/connection activation, BlueZ, PipeWire/WirePlumber, display/night mode and theme controls pending |
| Overview | Tensorland workspaces/windows, activation, move and close operations, per-output presentation | Bounded Compio `GetOverview` polling, explicit pending/unavailable/failed states, retained workspace/window geometry, per-output surface lifecycle, pointer hit testing, `ActivateView`/`SetWorkspace`, drag-to-workspace `MoveViewToWorkspace`, per-card close controls, and Escape dismissal implemented; richer text/icon cards pending |
| Lock/session | `ext-session-lock`, PAM authentication and lock-safe notifications; independently requested by Tensor Idle | Surface placeholder only; idle policy extracted to `tensor-idle` |
| Media and clipboard | MPRIS controls and metadata; MIME-aware clipboard history with bounded image previews | Bounded MPRIS discovery, typed metadata/capabilities/position/duration, stable active-player selection, signal-driven retained state, transport controls, playback OSD progress, versioned `tensor-msg shell media`, and hot-reloadable Tensorland KDL media-key response implemented; media text/art rendering and clipboard pending |
| System monitoring | CPU, memory, GPU, disk, network, battery and bounded process inspection | Not started |
| Theme and settings | Material token model, light/dark and wallpaper-derived colors, motion/typography and persisted settings | Typed Shell KDL, Compio runtime reload, panel ordering, layout, media policy, launcher argv, and Tensorland endpoint application implemented; settings UI and product schema work extracted to `tensor-settings`, theme/motion keys pending |
| Extensibility | Stable typed widget/service contracts; external plugins only after permission and lifecycle policy exists | Not started |

## Delivery Order

1. Complete panel text/icon pipelines and tray/status services on the
   implemented hot-reloadable geometry, hit testing, pointer/touch actions,
   Tensorland snapshot, and shared multi-output Vulkan presentation lifecycle.
2. Complete notifications end to end: value model, freedesktop service,
   popup/center scene building, actions and persistence.
3. Extend the connected standalone Tensor Launcher with file, window,
   calculator and command providers without blocking the UI thread.
4. Reuse the implemented typed UPower snapshot in Tensor Idle and the completed
   control-center power meter; extend the implemented NetworkManager details
   snapshot and Wi-Fi radio action with a credential-complete connection flow,
   then connect BlueZ and PipeWire/WirePlumber through equivalent retained
   adapters. Extend the shared MPRIS snapshot with media text/art rendering.
   Preserve explicit pending,
   unavailable, failed and attention states.
5. Implement `ext-session-lock` as a protocol-correct workflow; layer-shell
   lock placeholders are not a completed lock screen.
6. Add clipboard, monitoring, theme/settings and vetted extensibility.

Cold discovery, parsing, image decoding, search indexing and shader preparation
stay outside frame work. Runtime services use bounded queues and retained
snapshots. Every surface uses `vulkan-renderer`; feature delivery must not add a
QML, wgpu, CPU-rendering, or legacy descriptor path.
