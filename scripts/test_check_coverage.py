"""Offline regression tests for the mandatory production coverage gate."""

import unittest

from check_coverage import check, executable_lines


def report(*counts):
    return "/repo/src/module.rs:\n" + "".join(
        f"{line:5}|{count:>7}|source\n" for line, count in enumerate(counts, 1)
    )


def aggregate(covered=90, count=100):
    return {"data": [{"totals": {"lines": {"covered": covered, "count": count}}}]}


class CoverageGateTests(unittest.TestCase):
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
