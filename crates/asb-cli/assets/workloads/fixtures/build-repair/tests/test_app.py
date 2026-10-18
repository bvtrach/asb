import unittest

from src.app import main


class AppTests(unittest.TestCase):
    def test_main(self):
        self.assertEqual(main(), "build fixture is healthy")


if __name__ == "__main__":
    unittest.main()
