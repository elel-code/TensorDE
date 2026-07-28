# Example Wallpapers

This directory contains small, redistributable `.gwpdir` examples.
Do not commit third-party Wallpaper Engine workshop assets here.

- `static-demo.gwpdir`: a minimal static image package using SVG assets.
- `slideshow-demo.gwpdir`: a minimal slideshow package that alternates two SVG
  slides.
- `shader-demo.gwpdir`: a minimal shader manifest with GLSL source metadata and
  a static fallback poster. Current renderers display the fallback.

Examples use canonical `manifest.gilder.json`. Hand-written `.gwpdir` packages
may use `manifest.gilder.toml`; packing normalizes them back to JSON.
