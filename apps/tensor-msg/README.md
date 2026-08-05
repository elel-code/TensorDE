# Tensor Msg

`tensor-msg` is the independently installable control client for Tensor
products. It deliberately does not link Tensor WM, Tensor Wallpaper's scene
engine, Vulkan, or a Wayland client runtime.

```text
tensor-msg land get-state
tensor-msg land set-layout scrolling
tensor-msg wallpaper status
tensor-msg wallpaper set background.gwp
```

Install only this application and `tensor-ipc` alongside the product being
controlled. In particular, a Wallpaper-only installation does not require the
`tensor` compositor binary.

Tensor Launcher and Tensor Greeter are short-lived clients without IPC
services. A `shell` target will be added only when Tensor Shell has a bounded,
versioned service endpoint.
