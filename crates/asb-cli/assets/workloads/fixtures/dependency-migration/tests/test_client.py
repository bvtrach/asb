import unittest

from src.client import request_label


class ClientTests(unittest.TestCase):
    def test_request_label(self):
        self.assertEqual(request_label("ok"), "request:ok")


if __name__ == "__main__":
    unittest.main()
