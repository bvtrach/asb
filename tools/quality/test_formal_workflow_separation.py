#!/usr/bin/env python3
# Copyright (C) Huawei Technologies Co., Ltd. 2026. All rights reserved.
# SPDX-License-Identifier: MIT
"""Contract tests keeping development CI separate from final-delivery TLC."""

from __future__ import annotations

import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
DEVELOPMENT = ROOT / ".github/workflows/formal.yml"
FINAL_DELIVERY = ROOT / ".github/workflows/final-delivery-formal.yml"


def validate_development_lane(text: str) -> None:
    if "run_temporal_models.sh" in text or "TLC" in text or "Alloy" in text:
        raise AssertionError("development workflow invokes the final-delivery formal suite")


def validate_final_delivery_lane(text: str) -> None:
    if text.count("formal/run_temporal_models.sh") != 1:
        raise AssertionError("final-delivery lane must contain exactly one formal invocation")


class FormalWorkflowSeparationTests(unittest.TestCase):
    def test_development_lane_retains_kani_and_loom_without_formal_suite(self) -> None:
        text = DEVELOPMENT.read_text(encoding="utf-8")
        self.assertIn("name: Development formal assurance", text)
        self.assertIn("name: Kani bounded proofs", text)
        self.assertIn("name: Loom and state models", text)
        self.assertNotIn("TLC", text)
        self.assertNotIn("Alloy", text)
        self.assertNotIn("run_temporal_models.sh", text)
        validate_development_lane(text)

    def test_final_delivery_is_the_single_explicit_formal_lane(self) -> None:
        text = FINAL_DELIVERY.read_text(encoding="utf-8")
        self.assertIn("name: Final delivery formal assurance (future)", text)
        self.assertIn("branches: [final-delivery]", text)
        self.assertEqual(text.count("formal/run_temporal_models.sh"), 1)
        self.assertEqual(text.count("name: Final delivery TLC and Alloy recovery models"), 1)
        self.assertNotIn("name: Kani bounded proofs", text)
        self.assertNotIn("name: Loom and state models", text)
        validate_final_delivery_lane(text)

    def test_all_workflows_have_only_one_temporal_invocation(self) -> None:
        workflows = list((ROOT / ".github/workflows").glob("*.y*ml"))
        invocation_count = sum(
            workflow.read_text(encoding="utf-8").count("formal/run_temporal_models.sh")
            for workflow in workflows
        )
        self.assertEqual(invocation_count, 1)
        self.assertEqual(
            [path.name for path in workflows if "formal/run_temporal_models.sh" in path.read_text(encoding="utf-8")],
            ["final-delivery-formal.yml"],
        )

    def test_development_required_checks_do_not_require_final_delivery_gate(self) -> None:
        settings = (ROOT / "tools/integration/repository_settings.py").read_text(
            encoding="utf-8"
        )
        policy = (ROOT / "tools/quality/repository_policy.py").read_text(encoding="utf-8")
        self.assertNotIn('"TLC and Alloy recovery models"', settings)
        self.assertNotIn('"TLC and Alloy recovery models"', policy)

    def test_negative_regression_rejects_tlc_added_to_development_lane(self) -> None:
        text = DEVELOPMENT.read_text(encoding="utf-8")
        mutated = text + "\n# accidental development formal invocation\nformal/run_temporal_models.sh\n"
        with self.assertRaises(AssertionError):
            validate_development_lane(mutated)

    def test_negative_regression_rejects_second_final_delivery_invocation(self) -> None:
        text = FINAL_DELIVERY.read_text(encoding="utf-8")
        mutated = text + "\nformal/run_temporal_models.sh\n"
        with self.assertRaises(AssertionError):
            validate_final_delivery_lane(mutated)


if __name__ == "__main__":
    unittest.main()
