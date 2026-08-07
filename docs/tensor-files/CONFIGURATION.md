# Tensor Files configuration

Tensor Files owns a typed KDL configuration at
`$XDG_CONFIG_HOME/tensor/files.kdl`. `TENSOR_FILES_CONFIG` overrides the path.
If neither an override nor an XDG/HOME configuration directory is available,
the read-only fallback path is `/etc/tensor/files.kdl`.

The complete schema is shown in
[`apps/tensor-files/examples/config.kdl`](../../apps/tensor-files/examples/config.kdl).
Every field is optional; an absent field uses the application default.

```kdl
places {
    sidebar {
        width 280.0 // logical pixels, greater than zero
        visible #true
    }
}

view {
    mode "compact" // "icons", "compact", or "details"
    show-hidden #false
    icons-preview-size 64 // each preview size is in 16..=256
    compact-preview-size 48
    details-preview-size 48
}

appearance {
    dark-mode #false
    background-blur #false
    background-opacity 1.0 // 0.0..=1.0
}
```

Tensor Files parses and writes this document through `tensor-kdl`. Unknown
nodes, malformed values, and unknown view modes are errors rather than silently
ignored settings. User actions such as changing the view mode, zoom level,
Places sidebar width, or appearance update the same file atomically. The
previous TSV settings format is not read as a compatibility path.

Tensor Settings registers Tensor Files as a KDL product at the same path and
uses the same typed validation rules before saving edits.
