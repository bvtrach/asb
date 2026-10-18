import unittest

from src.parser import parse_path


class ParserTests(unittest.TestCase):
    def test_regular_path(self):
        self.assertEqual(parse_path("a/b"), ["a", "b"])

    def test_trailing_separator_is_ignored(self):
        self.assertEqual(parse_path("a/b/"), ["a", "b"])


if __name__ == "__main__":
    unittest.main()
