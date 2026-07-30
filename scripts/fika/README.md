# Fika scripts

Run these paths from the TensorDE workspace root.

## Static and installation checks

- `check-install-data.sh` stages metadata and checks the installation scripts.
- `check-runtime-integration.sh` verifies installed D-Bus, portal, Polkit, and
  desktop integration.
- `check-native-wayland-smoke.sh` runs the optional native Wayland capability
  smoke test.
- `install-data.sh` installs Fika integration metadata into `DESTDIR`/`PREFIX`.

## Performance analyzers

- `analyze-item-view-perf.sh` and `check-item-view-perf-analyzer.sh`
- `analyze-places-perf.sh` and `check-places-perf-analyzer.sh`
- `analyze-vulkan-frame-log.sh` and `check-vulkan-frame-log-analyzer.sh`
- `check-item-view-runtime-log.sh`
- `compare-item-image-renderers.sh`
- `summarize-item-view-renderer-evidence.sh`

The `check-*-analyzer.sh` scripts test their matching parser with synthetic
logs. Vulkan frame logs report presentation latency and retained-work state;
the analyzer can scope evidence by view mode or redraw reason.

## Runtime evidence

- `run-retained-renderer-evidence.sh` captures item-view and Places logs in a
  real desktop session.
- `dialog-lifecycle-smoke.sh` exercises parented dialog lifecycle behavior in a
  real desktop session.
