"""Canonical monorepo paths for Gilder development tools."""

from pathlib import Path


WORKSPACE_ROOT = Path(__file__).resolve().parents[2]
GILDER_ROOT = WORKSPACE_ROOT / "apps/gilder"
ARTIFACTS_ROOT = WORKSPACE_ROOT / "artifacts/gilder"
DOCS_ROOT = WORKSPACE_ROOT / "docs/gilder"
REFERENCES_ROOT = WORKSPACE_ROOT / "references/gilder"
REVERSE_ENGINEERED_ROOT = WORKSPACE_ROOT / "reverse-engineered/gilder"
WALLPAPER_DISTRIBUTION = (
    ARTIFACTS_ROOT / "wallpaper-engine-workshop/steamcmd-root/distribution"
)
