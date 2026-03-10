# status-transition-guard

## What it teaches

- Pre-hook validation with `issue:status:changing`
- Returning structured validation errors
- Making transition policy configurable from TOML

## Hooks

- `issue:status:changing`

## Config

`examples/status-transition-guard/.fp/config.toml`:

```toml
[extensions.status-transition-guard]
allow_backlog = "todo"
allow_todo = "in-progress,backlog"
allow_in_progress = "done,todo"
allow_done = "todo"
```

## Quick test

1. Copy `.fp/extensions/status-transition-guard.ts` into your project.
2. Add the config block under your project `.fp/config.toml`.
3. Try a valid transition (for example `todo -> in-progress`) and confirm it succeeds.
4. Try an invalid transition (for example `backlog -> done`) and confirm it is rejected.
