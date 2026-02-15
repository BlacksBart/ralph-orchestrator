#!/bin/bash
#
# Run all unit tests for Ralph hat config parsing
#

set -euo pipefail

# Colors for output
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}Running Ralph Hat Config Parsing Unit Tests${NC}"
echo "==========================================="
echo ""

# Track overall results
ALL_PASSED=true

# Run shell-based tests
echo -e "${YELLOW}Running shell-based tests...${NC}"
if ./tests/test_hat_config_parsing.sh; then
    echo -e "${GREEN}Shell tests passed${NC}"
else
    echo -e "${RED}Shell tests failed${NC}"
    ALL_PASSED=false
fi

echo ""
echo -e "${YELLOW}Running Python-based tests...${NC}"

# Check if Python 3 is available
if command -v python3 &> /dev/null; then
    if python3 ./tests/test_hat_config_parsing.py; then
        echo -e "${GREEN}Python tests passed${NC}"
    else
        echo -e "${RED}Python tests failed${NC}"
        ALL_PASSED=false
    fi
else
    echo -e "${YELLOW}Python 3 not found, skipping Python tests${NC}"
fi

echo ""
echo "==========================================="

if $ALL_PASSED; then
    echo -e "${GREEN}All test suites passed!${NC}"
    exit 0
else
    echo -e "${RED}Some test suites failed${NC}"
    exit 1
fi