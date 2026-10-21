#!/usr/bin/env python3
"""Enforce coverage without allowing inline test bodies to inflate the gate.

LLVM's non-test executable map supplies the denominator. Counts come from the
complete unit/integration-test report, so unit tests still cover real code.
No production file is excluded. The ordinary LLVM aggregate must also pass.
"""

import argparse
import json
from pathlib import Path
import re


def executable_lines(report):
    """Parse LLVM's text format, unioning duplicate generic instantiations."""
    result = {}
    filename = None
    for text in report.splitlines():
        if text.startswith("/") and text.endswith(":"):
            filename = text[:-1]
        match = re.match(r"^\s*(\d+)\|\s*(\d+(?:\.\d+)?[kMGT]?)\|", text)
        if filename and match:
            key = (filename, int(match[1]))
            result[key] = result.get(key, False) or float(
                match[2].rstrip("kMGT")
            ) > 0
    return result


def check(all_report, production_report, aggregate, minimum):
    """Return both measurements and fail closed on incomplete/missing maps."""
    all_lines = executable_lines(all_report)
    production_lines = executable_lines(production_report)
    if not production_lines:
        raise ValueError("production executable coverage map is empty")
    missing = production_lines.keys() - all_lines.keys()
    if missing:
        raise ValueError(f"test coverage report omits {len(missing)} production lines")
    files = {}
    for key in production_lines:
        counts = files.setdefault(key[0], {"covered": 0, "count": 0})
        counts["count"] += 1
        counts["covered"] += all_lines[key]
    covered = sum(value["covered"] for value in files.values())
    count = len(production_lines)
    totals = aggregate["data"][0]["totals"]["lines"]
    if not totals["count"] or not 0 <= totals["covered"] <= totals["count"]:
        raise ValueError("invalid aggregate coverage denominator/count")
    production_percent = 100 * covered / count
    aggregate_percent = 100 * totals["covered"] / totals["count"]
    return {
        "minimum_percent": minimum,
        "production": {"covered": covered, "count": count, "percent": production_percent},
        "aggregate": {**totals, "percent": aggregate_percent},
        "files": files,
        "passed": production_percent >= minimum and aggregate_percent >= minimum,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--all-lines", required=True, type=Path)
    parser.add_argument("--production-lines", required=True, type=Path)
    parser.add_argument("--aggregate", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    result = check(
        args.all_lines.read_text(), args.production_lines.read_text(),
        json.loads(args.aggregate.read_text()), 90,
    )
    args.output.write_text(json.dumps(result, indent=2) + "\n")
    for name in ("production", "aggregate"):
        counts = result[name]
        print(f"{name}: {counts['covered']}/{counts['count']} lines "
              f"({counts['percent']:.2f}%; required 90.00%)")
    for path, counts in sorted(
        result["files"].items(), key=lambda item: item[1]["count"] - item[1]["covered"], reverse=True
    )[:20]:
        print(f"uncovered: {counts['count'] - counts['covered']:5} {path}")
    return 0 if result["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
