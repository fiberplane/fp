# hello-hooks

## What it teaches

- Smallest useful extension shape
- How to register multiple hooks
- How to read extension config

## Hooks

- `issue:created`
- `issue:status:changed`
- `comment:created`

## Config

`examples/hello-hooks/.fp/config.toml`:

```toml
[extensions.hello-hooks]
greeting_prefix = "👋"
```

## Quick test

1. Copy `.fp/extensions/hello-hooks.ts` into your project.
2. Add the config block under your project `.fp/config.toml`.
3. Run an action that creates an issue, changes status, and adds a comment.
4. Confirm the extension logs each event.
