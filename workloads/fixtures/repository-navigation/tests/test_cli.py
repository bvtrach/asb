import unittest

from src.cli import describe


class CliTests(unittest.TestCase):
    def test_known_entry(self):
        self.assertEqual(describe("alpha"), "first")


if __name__ == "__main__":
    unittest.main()
