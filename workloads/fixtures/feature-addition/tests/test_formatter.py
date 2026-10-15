import unittest

from src.formatter import format_items


class FormatterTests(unittest.TestCase):
    def test_default_order(self):
        self.assertEqual(format_items(["a", "b"]), "a, b")


if __name__ == "__main__":
    unittest.main()
