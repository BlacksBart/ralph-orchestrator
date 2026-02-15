  PR: Per-Hat Model Configuration for Ralph Orchestrator

  Title: Add per-hat model and backend configuration

  Description:

  Problem

  Ralph orchestrator's hat-based workflows (e.g., pdd-to-code-assist) currently use a single
   global model for all hats. This limits cost-optimization and performance strategies like:
  - Using Opus for reasoning-heavy hats (Inquisitor, Architect, Design Critic)
  - Using Sonnet for fast implementation hats (Builder, Validator, Committer)

  Solution

  Extend hat configuration to support optional per-hat model and backend overrides:

  hats:
    inquisitor:
      name: "Inquisitor"
      triggers: ["design.start"]
      publishes: ["question.asked"]
      model: "claude-opus-4-20250514"  # Override: use Opus for reasoning
      backend: "claude"
      instructions: |
        You are the Inquisitor...

    builder:
      name: "Builder"
      triggers: ["tasks.ready"]
      publishes: ["implementation.ready"]
      model: "claude-sonnet-4-5-20250929"  # Override: use Sonnet for speed
      backend: "claude"
      instructions: |
        You are the Builder...

    # Other hats without model override inherit global default
    validator:
      name: "Validator"
      triggers: ["implementation.ready"]
      publishes: ["validation.passed", "validation.failed"]
      # No model specified → uses global cli.model

  Implementation Details

  1. Config schema change (ralph-core/src/config.rs):
    - Add optional model: Option<String> field to Hat struct
    - Add optional backend: Option<String> field to Hat struct
    - Maintain backward compatibility: missing fields inherit global defaults
  2. Hat execution (ralph-core/src/event_loop.rs):
    - When invoking a hat's backend, check if hat has model override
    - Pass -c cli.model=<hat_model> to the backend CLI invocation
    - Fall back to global cli.model if hat has no override
  3. CLI behavior:
    - Global --model flag still works (sets default for all hats)
    - Per-hat model field overrides global setting
    - ralph run -c builtin:pdd-to-code-assist --model sonnet applies Sonnet to all hats
  except those with explicit overrides
  4. Documentation updates:
    - Add hat-level model configuration examples to docs/guide/configuration.md
    - Update presets (pdd-to-code-assist, tdd-red-green, code-assist) with Opus/Sonnet split
   recommendations

  Cost & Performance Impact

  - Cost savings: ~40-50% reduction when using Sonnet for implementation hats (Sonnet is
  ~half the cost of Opus)
  - Speed improvement: Sonnet is faster for code generation; Opus is better for planning
  - No breaking changes: Hats without model overrides work exactly as before

  Testing

  - Unit tests: Verify hat config parsing with/without model override
  - Integration tests: Run a preset with mixed models, assert correct model is invoked per
  hat
  - E2E test: pdd-to-code-assist with Opus reasoning + Sonnet implementation

  Backward Compatibility

  ✅ Fully backward compatible — existing configs work unchanged. Hat model and backend are
  optional fields.