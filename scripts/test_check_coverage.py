"""Offline regression tests for the mandatory production coverage gate."""

import unittest
import os
from pathlib import Path
import subprocess
import tempfile

from check_coverage import check, executable_lines


def report(*counts):
    return "/repo/src/module.rs:\n" + "".join(
        f"{line:5}|{count:>7}|source\n" for line, count in enumerate(counts, 1)
    )


def aggregate(covered=90, count=100):
    return {"data": [{"totals": {"lines": {"covered": covered, "count": count}}}]}


class CoverageGateTests(unittest.TestCase):
    def test_real_compiler_preserves_never_called_production_function_in_map(self):
        # Test LLVM's actual dead-code-map behavior, not just file presence or a
        # hand-written report. This fixture is outside the product coverage map.
        repository = Path(__file__).resolve().parent.parent
        target = repository / "target"
        target.mkdir(exist_ok=True)
        with tempfile.TemporaryDirectory(prefix="coverage-checker-", dir=target) as name:
            root = Path(name)
            source = root / "fixture.rs"
            source.write_text(
                "#![allow(dead_code)]\n"
                "#[inline(never)]\n"
                "fn never_called(x: u64) -> u64 {\n"
                "    x + 42\n"
                "}\n"
                "fn main() { println!(\"called\"); }\n"
            )
            binary = root / "fixture"
            subprocess.run(["rustc", "-C", "instrument-coverage", "-C", "opt-level=0",
                            "--crate-name", "coverage_checker_fixture", str(source),
                            "-o", str(binary)], check=True, capture_output=True)
            profile = root / "fixture.profraw"
            subprocess.run([str(binary)], env={**os.environ,"LLVM_PROFILE_FILE":str(profile)},
                           check=True, capture_output=True)
            sysroot = subprocess.check_output(["rustc","--print","sysroot"], text=True).strip()
            version = subprocess.check_output(["rustc","-vV"], text=True)
            host = next(line.removeprefix("host: ") for line in version.splitlines()
                        if line.startswith("host: "))
            tools = Path(sysroot) / "lib/rustlib" / host / "bin"
            merged = root / "fixture.profdata"
            subprocess.run([str(tools / "llvm-profdata"),"merge","-sparse",str(profile),
                            "-o",str(merged)], check=True, capture_output=True)
            report_text = subprocess.check_output([
                str(tools / "llvm-cov"),"show",f"-instr-profile={merged}",str(binary),
                "-show-line-counts-or-regions=false","-show-line-counts"
            ],text=True)
            # A single-file LLVM show omits its filename header.
            lines = executable_lines(f"{source}:\n" + report_text)
            self.assertIn((str(source),4),lines,"uncalled body must remain in denominator")
            self.assertFalse(lines[(str(source),4)])
            self.assertTrue(lines[(str(source),6)])

    def test_test_bodies_cannot_inflate_production_coverage(self):
        all_lines = report("1", "0", *(["1"] * 98))
        production = report("1", "0")
        result = check(all_lines, production, aggregate(99), 90)
        self.assertFalse(result["passed"])
        self.assertEqual(result["production"]["percent"], 50)

    def test_exact_threshold_passes_but_fraction_below_fails(self):
        production = report(*(["0"] * 1000))
        self.assertTrue(check(report(*(["1"] * 900 + ["0"] * 100)), production,
                              aggregate(), 90)["passed"])
        self.assertFalse(check(report(*(["1"] * 899 + ["0"] * 101)), production,
                               aggregate(), 90)["passed"])

    def test_aggregate_must_pass_independently(self):
        self.assertFalse(check(report("1"), report("1"), aggregate(89), 90)["passed"])

    def test_missing_or_empty_maps_fail_closed(self):
        for all_lines, production in [("", ""), ("", report("1"))]:
            with self.assertRaises(ValueError):
                check(all_lines, production, aggregate(), 90)

    def test_instantiation_counts_union_and_blank_source_is_not_executable(self):
        lines = executable_lines(report("0", "", "1.2k") + report("2", "", "0"))
        self.assertEqual(lines, {("/repo/src/module.rs", 1): True,
                                 ("/repo/src/module.rs", 3): True})

    def test_invalid_aggregate_fails_closed(self):
        for value in (aggregate(0, 0), aggregate(2, 1), aggregate(-1)):
            with self.assertRaises(ValueError):
                check(report("1"), report("1"), value, 90)


if __name__ == "__main__":
    unittest.main()
