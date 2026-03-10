# backlog-researcher

## What it teaches

- Triggering async automation on issue creation
- Executing external CLIs safely with timeout handling
- Posting extension-generated comments

## Hooks

- `issue:created`

## Config

`examples/backlog-researcher/.fp/config.toml`:

```toml
[extensions.backlog-researcher]
model = "sonnet"
timeout_seconds = "120"
trigger_statuses = "backlog,todo"
```

## Quick test

1. Copy `.fp/extensions/backlog-researcher.ts` into your project.
2. Add the config block under your project `.fp/config.toml`.
3. Ensure `claude` is available in your PATH.
4. Create a new issue in `backlog` or `todo` with a short description.
5. Confirm the extension posts a "researching" comment, then a follow-up research summary.
