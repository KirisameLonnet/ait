#!/usr/bin/env python3
"""Require at least 95% line coverage in every Cargo workspace member."""

import argparse
import json
import subprocess
from pathlib import Path


def crate_totals(report, metadata):
    members = set(metadata["workspace_members"])
    packages = {
        package["name"]: Path(package["manifest_path"]).resolve().parent
        for package in metadata["packages"]
        if package["id"] in members
    }
    totals = {name: {"covered": 0, "count": 0} for name in packages}
    seen = set()
    for unit in report["data"]:
        for source in unit["files"]:
            path = Path(source["filename"]).resolve()
            if path in seen:
                raise ValueError(f"Duplicate source in coverage report: {path}")
            seen.add(path)
            owners = [(name, root) for name, root in packages.items() if path.is_relative_to(root)]
            if not owners:
                continue
            name, _ = max(owners, key=lambda owner: len(owner[1].parts))
            lines = source["summary"]["lines"]
            if not 0 <= lines["covered"] <= lines["count"]:
                raise ValueError(f"Invalid line counts for {path}")
            for key in ("covered", "count"):
                totals[name][key] += lines[key]
    return totals


def below_threshold(lines):
    return lines["count"] == 0 or lines["covered"] * 100 < lines["count"] * 95


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("report", type=Path, help="cargo llvm-cov JSON report")
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--no-deps", "--format-version", "1", "--locked", "--offline"],
        cwd=root, text=True,
    ))
    totals = crate_totals(json.loads(args.report.read_text()), metadata)
    failed = False
    for name, lines in sorted(totals.items()):
        missing = lines["count"] == 0
        percent = lines["covered"] * 100 / lines["count"] if not missing else 0
        rejected = below_threshold(lines)
        failed |= rejected
        detail = "missing coverage" if missing else f"{percent:.4f}% ({lines['covered']}/{lines['count']})"
        print(f"{'FAIL' if rejected else 'PASS'} {name}: {detail}")
    return int(failed)


if __name__ == "__main__":
    raise SystemExit(main())
