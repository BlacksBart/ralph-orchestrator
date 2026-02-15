# Ralph Hat Config Parsing Unit Tests

This directory contains unit tests for Ralph's per-hat model and backend configuration feature.

## Overview

These tests validate that Ralph correctly parses and handles hat configurations with optional model and backend overrides. The tests ensure backward compatibility while verifying the new functionality works as expected.

## Test Coverage

### Configuration Scenarios

1. **Model and Backend Override**: Hats with both `model` and `backend` fields specified
2. **Model Only Override**: Hats with only `model` field, inheriting global backend
3. **Backend Only Override**: Hats with only `backend` field, inheriting global model
4. **No Overrides**: Hats using global defaults for both model and backend
5. **Mixed Configurations**: Workflows with multiple hats using different model configurations
6. **Model Shorthand**: Testing shorthand model names (e.g., "opus", "sonnet", "haiku")
7. **Empty/Null Fields**: Handling of empty strings and null values
8. **Invalid Models**: Proper failure for non-existent model names
9. **Complex Workflows**: Multi-stage workflows with different models per stage

### Test Files

- `test_hat_config_parsing.sh` - Shell-based tests using Ralph's dry-run validation
- `test_hat_config_parsing.py` - Python-based tests with detailed validation
- `run_all_tests.sh` - Runs all test suites

## Running Tests

### Run All Tests
```bash
./tests/run_all_tests.sh
```

### Run Individual Test Suites
```bash
# Shell tests only
./tests/test_hat_config_parsing.sh

# Python tests only
python3 ./tests/test_hat_config_parsing.py
```

## Expected Behavior

According to the Ralph v2.5.0 implementation:

1. **Model Resolution**:
   - If hat has `model` field → use hat's model
   - Otherwise → use global `cli.model`
   - Model shorthands are resolved (opus → claude-opus-4-20250514)

2. **Backend Resolution**:
   - If hat has `backend` field → use hat's backend
   - Otherwise → use global `cli.backend`

3. **Validation**:
   - Invalid model names should cause config validation to fail
   - Empty model fields should fall back to global default
   - All existing configs without model/backend fields remain valid

## Test Output

Tests output results in color:
- 🟢 **GREEN**: Test passed
- 🔴 **RED**: Test failed
- 🟡 **YELLOW**: Information/warnings

Each test suite provides a summary showing total tests, passed, and failed counts.