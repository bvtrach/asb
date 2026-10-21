"""Credential-isolated subprocess fixtures retain only coverage instrumentation."""
import os
import unittest
from unittest.mock import patch


def coverage_env(environment):
    result = dict(environment)
    if "LLVM_PROFILE_FILE" in os.environ:
        result["LLVM_PROFILE_FILE"] = os.environ["LLVM_PROFILE_FILE"]
    return result


class EnvironmentTests(unittest.TestCase):
    def test_retains_only_explicit_environment_and_profile(self):
        with patch.dict(os.environ, {"LLVM_PROFILE_FILE": "fixture-%p.profraw",
                                     "OPENROUTER_API_KEY": "not-inherited"}, clear=True):
            source = {"PATH": "/bin", "HOME": "fixture-home"}
            self.assertEqual(coverage_env(source), {**source, "LLVM_PROFILE_FILE": "fixture-%p.profraw"})
            self.assertNotIn("LLVM_PROFILE_FILE", source)

    def test_normal_execution_has_no_synthetic_profile_variable(self):
        with patch.dict(os.environ, {"OPENROUTER_API_KEY": "not-inherited"}, clear=True):
            self.assertEqual(coverage_env({"PATH": "/bin"}), {"PATH": "/bin"})


if __name__ == "__main__":
    unittest.main()
