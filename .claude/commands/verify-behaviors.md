# Verify Behavioral Implementation

Run all behavioral verifications from `specs/behaviors.yaml` and report status.

## Process

1. **Build the project** (if not already built):
   ```bash
   cargo build --release 2>/dev/null || cargo build
   ```

2. **Set up PATH** to include the ralph binary:
   ```bash
   export PATH="./target/release:./target/debug:$PATH"
   ```

3. **Parse `specs/behaviors.yaml`** and extract all behaviors

4. **For each spec's behaviors**:
   - Run the verification command
   - Record pass (exit 0) or fail (exit non-zero)
   - Capture any error output for failures

5. **Generate a report** showing:
   - Total behaviors checked
   - Passing behaviors (implemented)
   - Failing behaviors (missing/broken)
   - Skipped behaviors (skip_ci: true)

## Output Format

```markdown
# Behavioral Verification Report

## Summary
- ✅ Passing: 24
- ❌ Failing: 3
- ⏭️ Skipped: 2

## Results by Spec

### interactive-mode.spec.md
- ✅ -i/--interactive flag exists
- ✅ -a/--autonomous flag exists
- ✅ --idle-timeout flag exists
- ❌ Double Ctrl+C handling (cannot verify in non-interactive)

### event-loop.spec.md
- ✅ run command exists
- ✅ resume command exists
...

## Failed Behaviors (Details)

### [interactive-mode] Double Ctrl+C handling
**Command:** `...`
**Error:** Cannot test interactive signal handling in automated context
**Recommendation:** Mark as skip_ci or verify manually

## Missing Behaviors

The following specs have no behaviors defined:
- benchmark-ux.spec.md (0 behaviors)
```

## Gap Analysis Integration

When running gap analysis:
1. First run `/verify-behaviors`
2. Only report as "missing" features that have FAILING behaviors
3. For specs with no behaviors, fall back to code search (with lower confidence)

## Important

- Run behaviors in isolation (one at a time)
- Use timeout for commands that might hang
- Don't let one failure stop verification of others
- Capture stderr for debugging failed checks
