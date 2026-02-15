# Makefile for Ralph Per-Hat Model Configuration Tests

.PHONY: test test-shell test-python clean help

# Default target
all: test

# Run all tests
test:
	@echo "Running all tests..."
	@./tests/run_all_tests.sh

# Run only shell tests
test-shell:
	@echo "Running shell tests..."
	@./tests/test_hat_config_parsing.sh

# Run only Python tests
test-python:
	@echo "Running Python tests..."
	@python3 ./tests/test_hat_config_parsing.py

# Clean temporary files
clean:
	@echo "Cleaning temporary files..."
	@rm -f /tmp/ralph-config-test-*
	@rm -f /tmp/test*.yml

# Show help
help:
	@echo "Available targets:"
	@echo "  make test        - Run all tests"
	@echo "  make test-shell  - Run shell-based tests only"
	@echo "  make test-python - Run Python-based tests only"
	@echo "  make clean       - Clean temporary test files"
	@echo "  make help        - Show this help message"