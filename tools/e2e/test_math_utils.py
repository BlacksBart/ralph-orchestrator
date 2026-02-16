"""Unit tests for math_utils module."""

import pytest
from math_utils import add_numbers


class TestAddNumbers:
    """Test suite for the add_numbers function."""

    def test_add_positive_integers(self):
        """Adding two positive integers returns their sum."""
        assert add_numbers(2, 3) == 5

    def test_add_negative_integers(self):
        """Adding two negative integers returns their sum."""
        assert add_numbers(-2, -3) == -5

    def test_add_mixed_signs(self):
        """Adding numbers with opposite signs works correctly."""
        assert add_numbers(-1, 1) == 0
        assert add_numbers(5, -3) == 2

    def test_add_with_zero(self):
        """Adding zero to a number returns the number unchanged."""
        assert add_numbers(0, 5) == 5
        assert add_numbers(5, 0) == 5
        assert add_numbers(0, 0) == 0

    def test_add_floats(self):
        """Adding floating-point numbers works correctly."""
        assert add_numbers(1.5, 2.5) == 4.0
        assert add_numbers(0.1, 0.2) == pytest.approx(0.3)

    def test_add_large_numbers(self):
        """Adding large numbers works correctly."""
        assert add_numbers(10**18, 10**18) == 2 * 10**18

    def test_type_coercion(self):
        """Mixed int and float arguments work correctly."""
        result = add_numbers(1, 2.5)
        assert result == 3.5
        assert isinstance(result, float)
