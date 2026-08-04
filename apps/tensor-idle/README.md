# Tensor Idle

Tensor Idle is an independent policy service, not part of Tensor Shell. It
models separate AC and battery deadlines for monitor power-off, session lock,
suspend, and post-lock monitor power-off. A zero timeout disables only that
stage. `respect-inhibitors` selects compositor-governed inhibitable idle
notifications versus input-only notifications.

Configuration is typed KDL at `$XDG_CONFIG_HOME/tensor/idle.kdl`; use
`TENSOR_IDLE_CONFIG` to select an explicit file. The current slice validates
the configuration and compiles a deterministic, bounded action plan through
`tensor-idle --check` (or `--check --battery`). `--check-wayland` performs a
real connection, registers every deadline, and rolls the objects back on exit;
`--observe` continuously reports deduplicated transitions ordered by policy
rather than wire arrival order. `--run-output-power` runs the same event loop
and executes only the completed monitor-power stage; lock and suspend remain
reported with `executed=false`.

When monitor-off policy is enabled, the runtime requires
`zwlr_output_power_manager_v1`, retains one control per live output, and exposes
an action boundary that turns every output off on idle and back on on resume.
Hotplug inherits the current policy, repeated events do not repeat requests,
and a failed control restores the remaining outputs before returning an explicit
error. DPMS does not remove or disable outputs in the compositor topology.

Normal service startup still fails explicitly until all action execution is
vertically complete. `ext-session-lock-v1`, power-source observation, and
logind suspend remain. Those capabilities are implemented directly rather than
routed through Tensor Shell or `tensor-msg`.
