from __future__ import annotations

import importlib.util
import sys
import unittest
from pathlib import Path


SCRIPT = Path(__file__).resolve().parents[1] / "analyze_render_perf.py"
SPEC = importlib.util.spec_from_file_location("analyze_render_perf", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
analyzer = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = analyzer
SPEC.loader.exec_module(analyzer)


def event(composition: str, **overrides: int) -> str:
    values = {
        "output_device": 1,
        "output_connector": 2,
        "serial": 3,
        "timeline": 4,
        "output_pixels": 24_000,
        "shadow_draws": 0,
        "shadow_pixel_upper_bound": 0,
        "backdrop_passes": 0,
        "backdrop_sample_pixels": 0,
        "backdrop_filter_pixels": 0,
        "backdrop_filter_texture_samples": 0,
        "backdrop_composite_pixel_upper_bound": 0,
        "backdrop_retained_intermediate_pixels": 0,
        "elapsed_us": 11,
    }
    values.update(overrides)
    fields = " ".join(f"{name}={value}" for name, value in values.items())
    return f"2026-08-03 INFO tensor: frame submit {fields} composition={composition}\n"


class ParseSampleTests(unittest.TestCase):
    def test_parses_ansi_wrapped_self_contained_event(self) -> None:
        line = "\x1b[32m" + event("direct-single-pass") + "\x1b[0m"

        sample = analyzer.parse_sample(line, "fixture:1")

        self.assertIsNotNone(sample)
        self.assertEqual(sample.composition, "direct-single-pass")
        self.assertEqual(sample.elapsed_us, 11)

    def test_ignores_other_tracing_events(self) -> None:
        self.assertIsNone(
            analyzer.parse_sample(
                "INFO tensor: renderer frame submitted to atomic KMS serial=1\n",
                "fixture:1",
            )
        )

    def test_rejects_legacy_elapsed_only_event(self) -> None:
        with self.assertRaisesRegex(analyzer.AnalysisError, "is missing serial"):
            analyzer.parse_sample(
                "INFO tensor: frame submit output_device=1 output_connector=2 elapsed_us=8",
                "fixture:1",
            )

    def test_rejects_duplicate_fields_in_one_event(self) -> None:
        with self.assertRaisesRegex(analyzer.AnalysisError, "duplicate field 'serial'"):
            analyzer.parse_sample(
                event("direct-single-pass") + " serial=99", "fixture:1"
            )

    def test_rejects_direct_event_with_hidden_backdrop_work(self) -> None:
        with self.assertRaisesRegex(analyzer.AnalysisError, "zero backdrop workload"):
            analyzer.parse_sample(
                event("direct-single-pass", backdrop_passes=1), "fixture:1"
            )

    def test_rejects_shadow_pixels_without_a_draw(self) -> None:
        with self.assertRaisesRegex(analyzer.AnalysisError, "shadow draw count"):
            analyzer.parse_sample(
                event("direct-single-pass", shadow_pixel_upper_bound=64), "fixture:1"
            )

    def test_rejects_incoherent_filter_work(self) -> None:
        with self.assertRaisesRegex(analyzer.AnalysisError, "two sample lanes"):
            analyzer.parse_sample(
                event(
                    "backdrop-multi-pass",
                    backdrop_passes=1,
                    backdrop_sample_pixels=256,
                    backdrop_filter_pixels=511,
                    backdrop_filter_texture_samples=4_608,
                    backdrop_composite_pixel_upper_bound=64,
                    backdrop_retained_intermediate_pixels=512,
                ),
                "fixture:1",
            )


class SummaryTests(unittest.TestCase):
    def test_rejects_input_without_frame_samples(self) -> None:
        with self.assertRaisesRegex(analyzer.AnalysisError, "no self-contained"):
            analyzer.read_samples([])

    def test_groups_paths_and_reports_nearest_rank_percentiles(self) -> None:
        direct = [
            analyzer.parse_sample(
                event("direct-single-pass", serial=index + 1, elapsed_us=elapsed),
                f"fixture:{index + 1}",
            )
            for index, elapsed in enumerate((5, 10, 20, 40))
        ]
        backdrop = analyzer.parse_sample(
            event(
                "backdrop-multi-pass",
                serial=5,
                elapsed_us=50,
                backdrop_passes=1,
                backdrop_sample_pixels=240,
                backdrop_filter_pixels=480,
                backdrop_filter_texture_samples=4_320,
                backdrop_composite_pixel_upper_bound=100,
                backdrop_retained_intermediate_pixels=480,
            ),
            "fixture:5",
        )

        result = analyzer.summarize([*direct, backdrop])

        self.assertEqual(result["schema"], "tensor-render-perf-v2")
        self.assertEqual(result["frames"], 5)
        direct_group = result["groups"]["direct-single-pass"]
        self.assertEqual(direct_group["elapsed_us"], {"p50": 10, "p95": 40, "p99": 40})
        multi_group = result["groups"]["backdrop-multi-pass"]
        self.assertEqual(multi_group["sample_localization_percent"], 1.0)
        self.assertEqual(multi_group["retained_capacity_percent"], 1.0)


if __name__ == "__main__":
    unittest.main()
