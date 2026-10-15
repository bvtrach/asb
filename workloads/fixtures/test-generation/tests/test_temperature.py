import unittest

from src.temperature import classify


class TemperatureTests(unittest.TestCase):
    def test_freezing(self):
        self.assertEqual(classify(-1), "freezing")

    def test_hot(self):
        self.assertEqual(classify(31), "hot")


if __name__ == "__main__":
    unittest.main()
