"""Canonical monorepo paths for Tensor Wallpaper development tools."""

from pathlib import Path


WORKSPACE_ROOT = Path(__file__).resolve().parents[2]
TENSOR_WALLPAPER_ROOT = WORKSPACE_ROOT / "apps/tensor-wallpaper"
ARTIFACTS_ROOT = WORKSPACE_ROOT / "artifacts/tensor-wallpaper"
DOCS_ROOT = WORKSPACE_ROOT / "docs/tensor-wallpaper"
REFERENCES_ROOT = WORKSPACE_ROOT / "references/tensor-wallpaper"
REVERSE_ENGINEERED_ROOT = WORKSPACE_ROOT / "reverse-engineered/tensor-wallpaper"
WALLPAPER_DISTRIBUTION = (
    ARTIFACTS_ROOT / "wallpaper-engine-workshop/steamcmd-root/distribution"
)
