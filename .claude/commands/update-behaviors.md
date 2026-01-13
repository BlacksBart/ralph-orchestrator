# Update Behavioral Verification Catalog

Perform a deep analysis of specs and implementation to update `specs/behaviors.yaml`.

## Process

1. **Build the project first** to ensure `ralph` binary is available:
   ```bash
   cargo build --release
   ```
   Add `./target/release` to PATH for verification commands.

2. **Read each spec** in `./specs/*.spec.md`

3. **For each spec**, identify verifiable behaviors:
   - **CLI flags**: Flags mentioned in the spec that should appear in `--help` output
   - **Config options**: YAML keys that should be recognized
   - **Runtime behaviors**: Commands that should succeed/fail in specific ways
   - **Output patterns**: Specific output text or formats

4. **Write verification commands** that:
   - Exit 0 when behavior IS implemented
   - Exit non-zero when behavior is NOT implemented
   - Are idempotent and fast (< 1 second)
   - Don't require external services or network
   - Use `ralph` commands, `grep`, `test`, etc.

5. **Update `specs/behaviors.yaml`** with the new behaviors

## Verification Command Patterns

### CLI Flag Existence
```yaml
- name: "-x/--example flag exists"
  type: cli-flag
  run: ralph run --help | grep -qE '^\s+-x, --example'
```

### Flag Accepts Value
```yaml
- name: "--example accepts string"
  type: cli-flag
  run: ralph run --example "test" --dry-run 2>&1 | grep -qv 'error'
```

### Mutually Exclusive Flags
```yaml
- name: "-x and -y are mutually exclusive"
  type: cli-flag
  run: ralph run -x -y --dry-run 2>&1 | grep -qi 'cannot be used with\|conflict'
```

### Config File Recognition
```yaml
- name: "config key recognized"
  type: config
  run: |
    echo 'event_loop: { example: true }' > /tmp/test.yml
    ralph run -c /tmp/test.yml --dry-run 2>&1 | grep -qv 'unknown field'
```

### Runtime Behavior
```yaml
- name: "events command works"
  type: runtime
  run: ralph events --help >/dev/null 2>&1
```

### Output Contains Pattern
```yaml
- name: "dry-run shows iterations"
  type: output
  run: ralph run --dry-run 2>&1 | grep -q 'Max iterations'
```

## Categories

Use these `type` values:
- `cli-flag`: Command-line argument existence/acceptance
- `config`: Configuration file parsing
- `runtime`: Command execution behavior
- `output`: Output format/content verification

## Skip CI

Add `skip_ci: true` for behaviors that:
- Require TTY (interactive mode tests)
- Need external services (Homebrew tap)
- Are destructive (clearing data)

## Output

Update `specs/behaviors.yaml` preserving:
- The file header comments
- Existing behaviors that still verify correctly
- Remove behaviors for deprecated features
- Add behaviors for new features found in specs

## Important

- **DO NOT** mark a behavior as implemented based on code search
- **DO** run the actual verification command to confirm it passes
- **DO** test both positive and negative cases where applicable
- Behaviors should test WHAT the user experiences, not HOW it's implemented
