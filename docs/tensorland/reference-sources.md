# Local Reference Worktrees

These repositories are intentionally kept as local, ignored worktrees. The parent repository
tracks only this manifest so large upstream histories are not accidentally committed.

| Project | URL | Branch | Snapshot |
| --- | --- | --- | --- |
| Niri | https://github.com/YaLTeR/niri | `main` | `7f26c3e804fb6ed458ef7fb0e3c794f14e0b3bc` |
| Hyprland | https://github.com/hyprwm/Hyprland | `main` | `275e27704a36d956fbdc28cec6399b8e298b06ca` |
| Nourish | https://github.com/y5-snowies/nourish | `upstream-integration` | `2ef4a74` |
| Bevy | https://github.com/bevyengine/bevy | `main` | `8997cf5` |

Refresh a worktree with:

```sh
git -C references/tensor/niri pull --ff-only
git -C references/tensor/hyprland pull --ff-only
git -C references/tensor/nourish pull --ff-only
git -C references/tensor/bevy pull --ff-only
```

## Hyprland review: 2026-08-04

The Hyprland worktree was fast-forwarded from `1a360623` (2026-07-21) to
`275e2770` (2026-08-04). The relevant changes were reviewed as behavioral and
profiling evidence, not as renderer code to copy:

- `6484f437` merges consecutive fence-ready/mailbox surface states while
  retaining FIFO barriers, and removes a linear syncobj resource lookup.
  Tensorland already commits synchronized surface trees as value transactions
  and keeps FIFO presentation ownership explicit; later state coalescing must
  preserve every callback, presentation feedback, release point, and
  double-buffered protocol dirty bit.
- `dd903837` retains fragmented damage unless its bounding extent costs at
  most twice the exact damaged area. Tensorland adopts that density rule at
  its fixed 64-region boundary and otherwise merges the pair with the least
  added area, avoiding the previous whole-output fallback for sparse damage.
- `94fe1706` deduplicates buffer fence references per monitor. Tensorland's
  frame extraction already deduplicates stable client image identities and
  retains retirement per output submission, so no second reference owner is
  introduced.
- `41b0fffd` avoids constructing an eventfd waiter when a DRM timeline point
  is already signaled and also removes repeated surface-box/fullscreen-policy
  queries. Tensorland imports client acquire points directly into Vulkan and
  uses one submitted io_uring poll only for exported frame-completion
  sync-files, so the eventfd optimization is not transplanted into the wrong
  ownership model. Repeated geometry/policy queries remain profiling targets.
- `1ea103b5` enables FP16 targets when supported. Tensorland already selects a
  typed `R16G16B16A16` linear working target for managed color and keeps HDR
  advertisement completion-gated on format plus KMS metadata support; support
  alone must not silently turn an SDR output into HDR.

Hyprland's OpenGL VAO split and compositor-specific transformer stack are not
applicable to Tensorland's retained Vulkan pipelines and semantic effect plan.
The transferable lesson is to retain alternate hot-path objects and choose
them by typed frame semantics rather than mutate one object between draws.
