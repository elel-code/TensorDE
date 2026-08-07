# Tensor Idle

Tensor Idle is an independent policy service, not part of Tensor Shell. It
models separate AC and battery deadlines for monitor power-off, session lock,
suspend, and post-lock monitor power-off. A zero timeout disables only that
stage. `respect-inhibitors` selects compositor-governed inhibitable idle
notifications versus input-only notifications.

Configuration is typed KDL at `$XDG_CONFIG_HOME/tensor/idle.kdl`; use
`TENSOR_IDLE_CONFIG` to select an explicit file. `tensor-idle --check` (or
`--check --battery`) validates it and compiles a deterministic, bounded action
plan. `--check-wayland` performs a real connection, registers every deadline,
and rolls the objects back on exit; `--observe` continuously reports
deduplicated transitions ordered by policy rather than wire arrival order.
`--run-output-power` executes only monitor power for focused diagnostics.

Normal startup, or `tensor-idle --run`, executes the complete selected plan.
Monitor power uses the Wayland output-power protocol. Lock and suspend use
`org.freedesktop.login1.Manager.LockSessions()` and `Suspend(false)` over a
caller-owned Compio/io_uring runtime and `tensor-dbus`; the async action
component does not create an executor or worker thread. Resume edges restore
monitor power but never repeat one-shot lock or suspend requests.

After logind accepts a lock request, Tensor Idle creates the configured
post-lock monitor-off notification. Its timeout starts when that new object is
created, so the deadline is measured from the successful lock action rather
than from the original user-idle timestamp. A resume edge cancels the
lock-cycle notification before its events are applied. Live AC/battery policy
changes register a replacement first and preserve the original lock timestamp;
failure leaves the previous notification active.

Normal startup observes UPower on the system bus and atomically re-registers
the idle deadlines when the machine changes between AC and battery power. The
UPower task owns a dedicated application-level Compio/io_uring runtime because
the Wayland loop is independently completion-driven; its state is coalesced to
one retained snapshot and wakes the Wayland loop through eventfd. If UPower is
absent, the service keeps the AC policy and retries without stopping idle
handling. `--battery` pins the battery policy for diagnostics.

The Wayland loop also checks the selected KDL path once per second. A valid
change registers a replacement monitor/output set before retiring the old one;
an invalid document or rejected replacement keeps the last active policy. A
live change that adds or removes logind action stages is reported as restart
required because it would change ownership of the already-created system
runtime.

When monitor-off policy is enabled, the runtime requires
`zwlr_output_power_manager_v1`, retains one control per live output, and exposes
an action boundary that turns every output off on idle and back on on resume.
Hotplug inherits the current policy, repeated events do not repeat requests,
and a failed control restores the remaining outputs before returning an explicit
error. DPMS does not remove or disable outputs in the compositor topology.

The lock surface remains a separate shell workflow; Tensor Idle owns policy and
asks logind to lock sessions rather than owning shell UI or routing the request
through `tensor-msg`.
