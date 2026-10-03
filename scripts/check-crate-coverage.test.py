"""Regression tests for the per-crate coverage gate, independent of Cargo execution."""

import importlib.util
import sys
import unittest
from pathlib import Path

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("coverage_gate", Path(__file__).with_name("check-crate-coverage.py"))
gate = importlib.util.module_from_spec(spec)
spec.loader.exec_module(gate)


class CoverageGateTests(unittest.TestCase):
    def test_threshold_uses_counts_without_rounding(self):
        self.assertFalse(gate.below_threshold({"covered": 95, "count": 100}))
        self.assertTrue(gate.below_threshold({"covered": 94999, "count": 100000}))
        self.assertTrue(gate.below_threshold({"covered": 0, "count": 0}))

    def test_missing_member_cannot_hide_behind_covered_dependencies(self):
        root = Path(__file__).resolve().parent
        metadata = {"workspace_members": ["a", "b"], "packages": [
            {"id": name, "name": name, "manifest_path": str(root / name / "Cargo.toml")}
            for name in ("a", "b", "dependency")
        ]}
        sources = [{"filename": str(root / name / "src/lib.rs"),
                    "summary": {"lines": {"covered": 100, "count": 100}}}
                   for name in ("a", "dependency")]
        totals = gate.crate_totals({"data": [{"files": sources}]}, metadata)
        self.assertEqual(totals, {"a": {"covered": 100, "count": 100}, "b": {"covered": 0, "count": 0}})
        with self.assertRaisesRegex(ValueError, "Duplicate source"):
            gate.crate_totals({"data": [{"files": sources + sources}]}, metadata)


if __name__ == "__main__":
    unittest.main()
