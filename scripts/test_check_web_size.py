"""The compressed size check is the bundle's only growth guard (#1078)."""
import unittest

from check_web_size import BUDGET_BYTES, gzip_size, over_budget


class WebSizeTests(unittest.TestCase):
    def test_gzip_size_is_the_wire_size(self):
        data = b'not a real wasm, but compressible' * 1000
        self.assertGreater(gzip_size(data), 0)
        self.assertLess(gzip_size(data), len(data))
        self.assertEqual(gzip_size(data), gzip_size(data), 'fixed mtime for comparisons')

    def test_the_budget_is_inclusive(self):
        self.assertFalse(over_budget(BUDGET_BYTES))
        self.assertTrue(over_budget(BUDGET_BYTES + 1))


if __name__ == '__main__':
    unittest.main()
