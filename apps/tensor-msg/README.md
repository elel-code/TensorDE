# Tensor Msg

`tensor-msg` is the independently installable control client for Tensor
products. It deliberately does not link Tensor WM, Tensor Wallpaper's scene
engine, Vulkan, or a Wayland client runtime.

```text
tensor-msg land get-state
tensor-msg land set-layout scrolling
tensor-msg shell media play-pause
tensor-msg wallpaper status
tensor-msg wallpaper set background.gwp
```

Install only this application and the shared IPC crates alongside the product
being controlled. In particular, a Wallpaper-only installation does not
require the `tensor` compositor binary.

Tensor Launcher and Tensor Greeter are short-lived clients without IPC
services. `tensor-msg shell media` calls the versioned `org.tensor.Shell1`
session-bus service and accepts `previous`, `play-pause`, or `next`. The running
Shell retains active-player selection, capability validation, and its
single-command action queue.
