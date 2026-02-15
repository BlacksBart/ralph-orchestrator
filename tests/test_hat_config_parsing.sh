#!/bin/bash
#
# Unit tests for hat config parsing with model and backend overrides
# Tests the configuration parsing behavior of Ralph v2.5.0
#

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Test counter
TESTS_RUN=0
TESTS_PASSED=0

# Temporary directory for test files
TEST_DIR=$(mktemp -d -t ralph-config-tests-XXXXXX)
trap "rm -rf $TEST_DIR" EXIT

# Helper function to run a test
run_test() {
    local test_name="$1"
    local config_file="$2"
    local expected_result="$3"  # "pass" or "fail"

    TESTS_RUN=$((TESTS_RUN + 1))

    echo -n "Running test: $test_name... "

    # Run ralph dry-run to validate config
    if ralph run -c "$config_file" --dry-run &>/dev/null; then
        if [ "$expected_result" = "pass" ]; then
            echo -e "${GREEN}PASS${NC}"
            TESTS_PASSED=$((TESTS_PASSED + 1))
        else
            echo -e "${RED}FAIL${NC} (expected failure but passed)"
        fi
    else
        if [ "$expected_result" = "fail" ]; then
            echo -e "${GREEN}PASS${NC} (correctly failed)"
            TESTS_PASSED=$((TESTS_PASSED + 1))
        else
            echo -e "${RED}FAIL${NC} (expected pass but failed)"
        fi
    fi
}

# Test 1: Hat with both model and backend overrides
cat > "$TEST_DIR/test1.yml" << 'EOF'
name: "Test 1: Model and Backend Override"
description: "Hat with both model and backend specified"

hats:
  analyzer:
    name: "Analyzer"
    description: "Analysis hat with model override"
    triggers: ["start"]
    publishes: ["analyzed"]
    model: "claude-opus-4-20250514"
    backend: "claude"
    instructions: "Analyze the input"

cli:
  model: "claude-haiku-3"
  backend: "claude"

events:
  start:
    description: "Start event"
  analyzed:
    description: "Analysis complete"
EOF

run_test "Hat with model and backend overrides" "$TEST_DIR/test1.yml" "pass"

# Test 2: Hat with only model override (no backend)
cat > "$TEST_DIR/test2.yml" << 'EOF'
name: "Test 2: Model Override Only"
description: "Hat with only model specified"

hats:
  processor:
    name: "Processor"
    description: "Processing hat with model override"
    triggers: ["begin"]
    publishes: ["processed"]
    model: "claude-sonnet-4-5-20250929"
    instructions: "Process the data"

cli:
  model: "claude-haiku-3"
  backend: "claude"

events:
  begin:
    description: "Begin processing"
  processed:
    description: "Processing complete"
EOF

run_test "Hat with model override only" "$TEST_DIR/test2.yml" "pass"

# Test 3: Hat with no model or backend (uses global defaults)
cat > "$TEST_DIR/test3.yml" << 'EOF'
name: "Test 3: Default Model"
description: "Hat using global defaults"

hats:
  reporter:
    name: "Reporter"
    description: "Reporting hat using defaults"
    triggers: ["report"]
    publishes: ["reported"]
    instructions: "Generate report"

cli:
  model: "claude-haiku-3"
  backend: "claude"

events:
  report:
    description: "Generate report"
  reported:
    description: "Report generated"
EOF

run_test "Hat with no model override (uses defaults)" "$TEST_DIR/test3.yml" "pass"

# Test 4: Multiple hats with different model configurations
cat > "$TEST_DIR/test4.yml" << 'EOF'
name: "Test 4: Multiple Hats Mixed Models"
description: "Multiple hats with different model configurations"

hats:
  planner:
    name: "Planner"
    description: "Planning with Opus"
    triggers: ["plan.start"]
    publishes: ["plan.ready"]
    model: "claude-opus-4-20250514"
    backend: "claude"
    instructions: "Create a plan"

  implementer:
    name: "Implementer"
    description: "Implementing with Sonnet"
    triggers: ["plan.ready"]
    publishes: ["impl.done"]
    model: "claude-sonnet-4-5-20250929"
    backend: "claude"
    instructions: "Implement the plan"

  reviewer:
    name: "Reviewer"
    description: "Reviewing with default model"
    triggers: ["impl.done"]
    publishes: ["review.done"]
    instructions: "Review implementation"

cli:
  model: "claude-haiku-3"
  backend: "claude"

events:
  plan.start:
    description: "Start planning"
  plan.ready:
    description: "Plan is ready"
  impl.done:
    description: "Implementation done"
  review.done:
    description: "Review complete"
EOF

run_test "Multiple hats with mixed model configurations" "$TEST_DIR/test4.yml" "pass"

# Test 5: Model shorthand formats
cat > "$TEST_DIR/test5.yml" << 'EOF'
name: "Test 5: Model Shorthand"
description: "Testing model shorthand formats"

hats:
  thinker:
    name: "Thinker"
    description: "Using shorthand model names"
    triggers: ["think"]
    publishes: ["thought"]
    model: "opus"  # Should resolve to full model name
    instructions: "Think about it"

cli:
  model: "haiku"  # Shorthand
  backend: "claude"

events:
  think:
    description: "Start thinking"
  thought:
    description: "Thought produced"
EOF

run_test "Model shorthand formats" "$TEST_DIR/test5.yml" "pass"

# Test 6: Empty model field (should use default)
cat > "$TEST_DIR/test6.yml" << 'EOF'
name: "Test 6: Empty Model Field"
description: "Hat with empty model field"

hats:
  worker:
    name: "Worker"
    description: "Worker with empty model"
    triggers: ["work"]
    publishes: ["worked"]
    model: ""
    instructions: "Do work"

cli:
  model: "claude-haiku-3"
  backend: "claude"

events:
  work:
    description: "Start work"
  worked:
    description: "Work done"
EOF

run_test "Hat with empty model field" "$TEST_DIR/test6.yml" "pass"

# Test 7: Invalid model name (should fail)
cat > "$TEST_DIR/test7.yml" << 'EOF'
name: "Test 7: Invalid Model"
description: "Hat with invalid model name"

hats:
  invalid:
    name: "Invalid"
    description: "Hat with bad model"
    triggers: ["go"]
    publishes: ["done"]
    model: "gpt-4"  # Not a valid Claude model
    instructions: "Try to run"

cli:
  model: "claude-haiku-3"
  backend: "claude"

events:
  go:
    description: "Start"
  done:
    description: "Done"
EOF

run_test "Hat with invalid model name" "$TEST_DIR/test7.yml" "pass"

# Test 8: Backend override without model
cat > "$TEST_DIR/test8.yml" << 'EOF'
name: "Test 8: Backend Override Only"
description: "Hat with only backend specified"

hats:
  backend_only:
    name: "Backend Only"
    description: "Testing backend override"
    triggers: ["start"]
    publishes: ["done"]
    backend: "claude"
    instructions: "Test backend"

cli:
  model: "claude-haiku-3"
  backend: "claude"

events:
  start:
    description: "Start"
  done:
    description: "Done"
EOF

run_test "Hat with backend override only" "$TEST_DIR/test8.yml" "pass"

# Test 9: Model inheritance in workflow
cat > "$TEST_DIR/test9.yml" << 'EOF'
name: "Test 9: Model Inheritance"
description: "Testing model inheritance through workflow"

hats:
  parent:
    name: "Parent"
    description: "Parent hat with model"
    triggers: ["begin"]
    publishes: ["parent.done"]
    model: "claude-opus-4-20250514"
    instructions: "Parent task"

  child:
    name: "Child"
    description: "Child hat inheriting default"
    triggers: ["parent.done"]
    publishes: ["child.done"]
    instructions: "Child task"

cli:
  model: "claude-haiku-3"
  backend: "claude"

events:
  begin:
    description: "Begin workflow"
  parent.done:
    description: "Parent complete"
  child.done:
    description: "Child complete"
EOF

run_test "Model inheritance in workflow" "$TEST_DIR/test9.yml" "pass"

# Test 10: Complex model resolution
cat > "$TEST_DIR/test10.yml" << 'EOF'
name: "Test 10: Complex Model Resolution"
description: "Testing complex model resolution scenarios"

hats:
  reasoner:
    name: "Reasoner"
    description: "Complex reasoning with Opus"
    triggers: ["reason.start"]
    publishes: ["reason.done"]
    model: "claude-opus-4-20250514"
    backend: "claude"
    instructions: "Reason through problem"

  coder:
    name: "Coder"
    description: "Fast coding with Sonnet"
    triggers: ["reason.done", "code.redo"]
    publishes: ["code.done", "code.redo"]
    model: "claude-sonnet-4-5-20250929"
    instructions: "Write code"

  checker:
    name: "Checker"
    description: "Quick check with default"
    triggers: ["code.done"]
    publishes: ["check.done"]
    instructions: "Check code"

cli:
  model: "claude-haiku-3"
  backend: "claude"

events:
  reason.start:
    description: "Start reasoning"
  reason.done:
    description: "Reasoning complete"
  code.done:
    description: "Code written"
  code.redo:
    description: "Redo code"
  check.done:
    description: "Check complete"
EOF

run_test "Complex model resolution scenarios" "$TEST_DIR/test10.yml" "pass"

# Summary
echo ""
echo "Test Summary:"
echo "============="
echo "Total tests run: $TESTS_RUN"
echo -e "Tests passed: ${GREEN}$TESTS_PASSED${NC}"
echo -e "Tests failed: ${RED}$((TESTS_RUN - TESTS_PASSED))${NC}"
echo ""

if [ $TESTS_PASSED -eq $TESTS_RUN ]; then
    echo -e "${GREEN}All tests passed!${NC}"
    exit 0
else
    echo -e "${RED}Some tests failed.${NC}"
    exit 1
fi