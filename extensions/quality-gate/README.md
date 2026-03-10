# quality-gate

## What it teaches

- Blocking status transitions with pre-hook validation
- Running project checks before allowing `done`
- Returning detailed validation messages to the caller

## Hooks

- `issue:status:changing`

## Config

`examples/quality-gate/.fp/config.toml`:

```toml
[extensions.quality-gate]
checks = "test,typecheck,lint"
```

Default check command mapping:
- `test` → `bun test`
- `typecheck` → `bun run typecheck`
- `lint` → `bun run lint`

## Quick test

1. Copy `.fp/extensions/quality-gate.ts` into your project.
2. Add the config block under your project `.fp/config.toml`.
3. Move an issue toward `done` and confirm configured checks run.
4. Introduce a failing check and confirm the transition is blocked with details.
