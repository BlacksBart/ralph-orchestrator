#!/usr/bin/env python3
"""
Unit tests for Ralph hat configuration parsing with model and backend overrides.
Tests configuration validation and model resolution behavior.
"""

import yaml
import json
import subprocess
import tempfile
import os
import sys
from pathlib import Path
from typing import Dict, Any, Optional, List, Tuple

# ANSI color codes
GREEN = '\033[0;32m'
RED = '\033[0;31m'
YELLOW = '\033[1;33m'
NC = '\033[0m'  # No Color


class TestResult:
    """Represents a single test result."""
    def __init__(self, name: str, passed: bool, message: str = ""):
        self.name = name
        self.passed = passed
        self.message = message


class HatConfigTester:
    """Tests Ralph hat configuration parsing."""

    def __init__(self):
        self.results: List[TestResult] = []
        self.temp_dir = tempfile.mkdtemp(prefix="ralph-config-test-")

    def __enter__(self):
        return self

    def __exit__(self, exc_type, exc_val, exc_tb):
        # Cleanup temp directory
        subprocess.run(["rm", "-rf", self.temp_dir], capture_output=True)

    def create_config(self, name: str, content: Dict[str, Any]) -> str:
        """Create a temporary config file and return its path."""
        config_path = os.path.join(self.temp_dir, f"{name}.yml")
        with open(config_path, 'w') as f:
            yaml.dump(content, f, default_flow_style=False)
        return config_path

    def validate_config(self, config_path: str) -> Tuple[bool, str]:
        """Validate a config file using ralph dry-run."""
        result = subprocess.run(
            ["ralph", "run", "-c", config_path, "--dry-run"],
            capture_output=True,
            text=True
        )
        return result.returncode == 0, result.stderr

    def test(self, name: str, config: Dict[str, Any], should_pass: bool = True):
        """Run a single test."""
        config_path = self.create_config(name.replace(" ", "_"), config)
        passed, error = self.validate_config(config_path)

        if should_pass and passed:
            self.results.append(TestResult(name, True))
        elif not should_pass and not passed:
            self.results.append(TestResult(name, True, "Correctly failed"))
        else:
            msg = f"Expected {'pass' if should_pass else 'fail'} but got {'pass' if passed else 'fail'}"
            if error:
                msg += f"\nError: {error}"
            self.results.append(TestResult(name, False, msg))

    def run_tests(self):
        """Run all test cases."""

        # Test 1: Basic hat with model and backend overrides
        self.test("Basic hat with model and backend", {
            "name": "Test Basic Override",
            "description": "Basic test",
            "hats": {
                "analyzer": {
                    "name": "Analyzer",
                    "description": "Analysis hat",
                    "triggers": ["start"],
                    "publishes": ["done"],
                    "model": "claude-opus-4-20250514",
                    "backend": "claude",
                    "instructions": "Analyze input"
                }
            },
            "cli": {
                "model": "claude-haiku-3",
                "backend": "claude"
            },
            "events": {
                "start": {"description": "Start"},
                "done": {"description": "Done"}
            }
        })

        # Test 2: Hat with only model override
        self.test("Hat with model only", {
            "name": "Test Model Only",
            "description": "Model override without backend",
            "hats": {
                "processor": {
                    "name": "Processor",
                    "description": "Processing hat with model override",
                    "triggers": ["begin"],
                    "publishes": ["end"],
                    "model": "claude-sonnet-4-5-20250929",
                    "instructions": "Process data"
                }
            },
            "cli": {
                "model": "claude-haiku-3",
                "backend": "claude"
            },
            "events": {
                "begin": {"description": "Begin"},
                "end": {"description": "End"}
            }
        })

        # Test 3: Hat with no overrides (uses defaults)
        self.test("Hat using defaults", {
            "name": "Test Defaults",
            "description": "No model or backend override",
            "hats": {
                "default_hat": {
                    "name": "Default Hat",
                    "description": "Hat using default model",
                    "triggers": ["go"],
                    "publishes": ["done"],
                    "instructions": "Use default model"
                }
            },
            "cli": {
                "model": "claude-haiku-3",
                "backend": "claude"
            },
            "events": {
                "go": {"description": "Go"},
                "done": {"description": "Done"}
            }
        })

        # Test 4: Multiple hats with different models
        self.test("Multiple hats mixed models", {
            "name": "Test Mixed Models",
            "description": "Different models per hat",
            "hats": {
                "thinker": {
                    "name": "Thinker",
                    "description": "Deep reasoning with Opus",
                    "triggers": ["think.start"],
                    "publishes": ["think.done"],
                    "model": "claude-opus-4-20250514",
                    "instructions": "Deep thinking"
                },
                "doer": {
                    "name": "Doer",
                    "description": "Fast implementation with Sonnet",
                    "triggers": ["think.done"],
                    "publishes": ["do.done"],
                    "model": "claude-sonnet-4-5-20250929",
                    "instructions": "Fast doing"
                },
                "checker": {
                    "name": "Checker",
                    "description": "Quick validation with default model",
                    "triggers": ["do.done"],
                    "publishes": ["check.done"],
                    "instructions": "Quick check with default"
                }
            },
            "cli": {
                "model": "claude-haiku-3",
                "backend": "claude"
            },
            "events": {
                "think.start": {"description": "Start thinking"},
                "think.done": {"description": "Thinking done"},
                "do.done": {"description": "Doing done"},
                "check.done": {"description": "Checking done"}
            }
        })

        # Test 5: Model shorthand
        self.test("Model shorthand formats", {
            "name": "Test Shorthand",
            "description": "Using model shortcuts",
            "hats": {
                "opus_hat": {
                    "name": "Opus Hat",
                    "description": "Testing opus shorthand",
                    "triggers": ["start"],
                    "publishes": ["done"],
                    "model": "opus",  # Shorthand
                    "instructions": "Use opus shorthand"
                }
            },
            "cli": {
                "model": "haiku",  # Shorthand
                "backend": "claude"
            },
            "events": {
                "start": {"description": "Start"},
                "done": {"description": "Done"}
            }
        })

        # Test 6: Empty model field
        self.test("Empty model field", {
            "name": "Test Empty Model",
            "description": "Empty model string",
            "hats": {
                "empty_model": {
                    "name": "Empty Model",
                    "description": "Hat with empty model string",
                    "triggers": ["go"],
                    "publishes": ["stop"],
                    "model": "",
                    "instructions": "Empty model field"
                }
            },
            "cli": {
                "model": "claude-haiku-3",
                "backend": "claude"
            },
            "events": {
                "go": {"description": "Go"},
                "stop": {"description": "Stop"}
            }
        })

        # Test 7: Invalid model (should fail)
        self.test("Invalid model name", {
            "name": "Test Invalid Model",
            "description": "Non-existent model",
            "hats": {
                "bad_model": {
                    "name": "Bad Model",
                    "description": "Hat with invalid model name",
                    "triggers": ["run"],
                    "publishes": ["fail"],
                    "model": "gpt-4",  # Invalid for Claude backend
                    "instructions": "Invalid model"
                }
            },
            "cli": {
                "model": "claude-haiku-3",
                "backend": "claude"
            },
            "events": {
                "run": {"description": "Run"},
                "fail": {"description": "Fail"}
            }
        })

        # Test 8: Backend override only
        self.test("Backend override only", {
            "name": "Test Backend Only",
            "description": "Backend without model",
            "hats": {
                "backend_hat": {
                    "name": "Backend Hat",
                    "description": "Testing backend override only",
                    "triggers": ["start"],
                    "publishes": ["end"],
                    "backend": "claude",
                    "instructions": "Backend override"
                }
            },
            "cli": {
                "model": "claude-haiku-3",
                "backend": "claude"
            },
            "events": {
                "start": {"description": "Start"},
                "end": {"description": "End"}
            }
        })

        # Test 9: Null model field
        self.test("Null model field", {
            "name": "Test Null Model",
            "description": "Model set to null",
            "hats": {
                "null_model": {
                    "name": "Null Model",
                    "description": "Hat with null model value",
                    "triggers": ["begin"],
                    "publishes": ["finish"],
                    "model": None,
                    "instructions": "Null model field"
                }
            },
            "cli": {
                "model": "claude-haiku-3",
                "backend": "claude"
            },
            "events": {
                "begin": {"description": "Begin"},
                "finish": {"description": "Finish"}
            }
        })

        # Test 10: Complex workflow with model inheritance
        self.test("Complex model inheritance workflow", {
            "name": "Test Complex Workflow",
            "description": "Multi-stage workflow with different models",
            "hats": {
                "architect": {
                    "name": "Architect",
                    "description": "Design with Opus",
                    "triggers": ["design.start"],
                    "publishes": ["design.ready"],
                    "model": "claude-opus-4-20250514",
                    "backend": "claude",
                    "instructions": "Create architecture"
                },
                "builder": {
                    "name": "Builder",
                    "description": "Build with Sonnet",
                    "triggers": ["design.ready", "build.retry"],
                    "publishes": ["build.done", "build.retry"],
                    "model": "claude-sonnet-4-5-20250929",
                    "instructions": "Build implementation"
                },
                "validator": {
                    "name": "Validator",
                    "description": "Validate with default",
                    "triggers": ["build.done"],
                    "publishes": ["valid", "invalid"],
                    "instructions": "Validate build"
                }
            },
            "cli": {
                "model": "claude-haiku-3",
                "backend": "claude"
            },
            "events": {
                "design.start": {"description": "Start design"},
                "design.ready": {"description": "Design ready"},
                "build.done": {"description": "Build complete"},
                "build.retry": {"description": "Retry build"},
                "valid": {"description": "Validation passed"},
                "invalid": {"description": "Validation failed"}
            }
        })

    def print_results(self):
        """Print test results summary."""
        total = len(self.results)
        passed = sum(1 for r in self.results if r.passed)
        failed = total - passed

        print("\nTest Results:")
        print("=" * 50)

        for result in self.results:
            status = f"{GREEN}PASS{NC}" if result.passed else f"{RED}FAIL{NC}"
            print(f"{result.name:<40} {status}")
            if result.message:
                print(f"  {result.message}")

        print("\nSummary:")
        print("-" * 50)
        print(f"Total tests: {total}")
        print(f"Passed: {GREEN}{passed}{NC}")
        print(f"Failed: {RED}{failed}{NC}")

        if failed == 0:
            print(f"\n{GREEN}All tests passed!{NC}")
            return 0
        else:
            print(f"\n{RED}Some tests failed.{NC}")
            return 1


def main():
    """Run the test suite."""
    print("Ralph Hat Config Parsing Unit Tests")
    print("===================================")

    with HatConfigTester() as tester:
        tester.run_tests()
        return tester.print_results()


if __name__ == "__main__":
    sys.exit(main())