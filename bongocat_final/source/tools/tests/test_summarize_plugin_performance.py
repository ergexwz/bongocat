"""Contracts for interpreting process samples at measurement boundaries."""
from pathlib import Path
import runpy
import unittest


MODULE = runpy.run_path(str(Path(__file__).resolve().parents[1] / "summarize-plugin-performance.py"))


class PerformanceSummaryTests(unittest.TestCase):
    def test_cpu_rates_are_weighted_by_actual_sampling_intervals(self):
        def row(cpu):
            return {"role": "host-service", "cpu_percent": str(cpu),
                    **{key: "0" for key in MODULE["METRICS"] if key != "cpu_percent"}}
        # Ignore the first warmup-crossing delta. The following 1-second and
        # 9-second intervals consume 10 and 0 CPU percentage-seconds.
        groups = MODULE["summarize_snapshots"]({10.0: [row(0)], 0.0: [row(999)], 1.0: [row(10)]})
        self.assertAlmostEqual(groups["host-service"]["cpu_percent"], 1.0)
        self.assertAlmostEqual(groups["total"]["cpu_percent"], 1.0)

    def test_cpu_delta_crossing_warmup_is_not_counted_as_idle_cpu(self):
        def row(cpu):
            return {"role": "host-service", "cpu_percent": str(cpu),
                    **{key: "0" for key in MODULE["METRICS"] if key != "cpu_percent"}}
        groups = MODULE["summarize_snapshots"]({1.0: [row(9.0)], 2.0: [row(0.1)], 3.0: [row(0.3)]})
        self.assertAlmostEqual(groups["host-service"]["cpu_percent"], 0.2)
        self.assertAlmostEqual(groups["total"]["cpu_percent"], 0.2)


if __name__ == "__main__":
    unittest.main()
