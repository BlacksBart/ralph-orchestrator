import unittest
from math_ops import add


class TestAdd(unittest.TestCase):
    def test_add_positive_numbers(self):
        self.assertEqual(add(2, 3), 5)

    def test_add_negative_numbers(self):
        self.assertEqual(add(-1, -1), -2)

    def test_add_mixed_numbers(self):
        self.assertEqual(add(-1, 5), 4)

    def test_add_zeros(self):
        self.assertEqual(add(0, 0), 0)

    def test_add_floats(self):
        self.assertAlmostEqual(add(1.5, 2.5), 4.0)


if __name__ == "__main__":
    unittest.main()
