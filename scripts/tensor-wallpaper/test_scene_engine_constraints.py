#!/usr/bin/env python3
"""Regression tests for the scene-engine repository constraint boundary."""

from __future__ import annotations

import sys
import tempfile
import unittest
from pathlib import Path


sys.path.insert(0, str(Path(__file__).resolve().parent))

from scene_engine_constraints import owned_rust_files


class OwnedRustFilesTests(unittest.TestCase):
    def test_ignored_reference_trees_are_outside_the_owned_source_boundary(self) -> None:
        with tempfile.TemporaryDirectory() as temporary_directory:
            root = Path(temporary_directory)
            expected = {
                root / "build.rs",
                root / "build" / "generated.rs",
                root / "src" / "engine.rs",
            }
            ignored_reference = (
                root / "references" / "bevy" / "crates" / "bevy_render" / "src" / "mod.rs"
            )
            for path in expected | {ignored_reference}:
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("// fixture\n", encoding="utf-8")

            self.assertEqual(set(owned_rust_files(root)), expected)


if __name__ == "__main__":
    unittest.main()
