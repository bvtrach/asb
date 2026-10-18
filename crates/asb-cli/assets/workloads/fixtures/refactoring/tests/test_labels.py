import unittest

from src.labels import label_pair, label_triplet


class LabelTests(unittest.TestCase):
    def test_pair(self):
        self.assertEqual(label_pair("a", "b"), "a:b")

    def test_triplet(self):
        self.assertEqual(label_triplet("a", "b", "c"), "a:b:c")


if __name__ == "__main__":
    unittest.main()
