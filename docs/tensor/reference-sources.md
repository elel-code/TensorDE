# Local Reference Worktrees

These repositories are intentionally kept as local, ignored worktrees. The parent repository
tracks only this manifest so large upstream histories are not accidentally committed.

| Project | URL | Branch | Snapshot |
| --- | --- | --- | --- |
| Niri | https://github.com/YaLTeR/niri | `main` | `7f26c3e804fb6ed458ef7fb0e3c794f14e0b3bc` |
| Hyprland | https://github.com/hyprwm/Hyprland | `main` | `1a3606234c59842340ad9a42baeeffe44a9d6cda` |
| Nourish | https://github.com/y5-snowies/nourish | `upstream-integration` | `2ef4a74` |
| Bevy | https://github.com/bevyengine/bevy | `main` | `8997cf5` |

Refresh a worktree with:

```sh
git -C references/tensor/niri pull --ff-only
git -C references/tensor/hyprland pull --ff-only
git -C references/tensor/nourish pull --ff-only
git -C references/tensor/bevy pull --ff-only
```
