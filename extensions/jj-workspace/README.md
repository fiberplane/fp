# jj-workspace

## What it teaches

- Coordinating pre-hook intent logs and post-hook side effects
- Automating jj bookmark/workspace lifecycle from issue statuses
- Running workspace bootstrap commands after workspace creation

## Hooks

- `issue:status:changing`
- `issue:status:changed`

## Config

`examples/jj-workspace/.fp/config.toml`:

```toml
[extensions.jj-lifecycle]
install_cmd = "bun install"
```

## Quick test

1. Copy `.fp/extensions/jj-lifecycle.ts` (and optional `nesting-limit.ts`) into your project.
2. Add the config block under your project `.fp/config.toml`.
3. Ensure `jj` is installed and your project is a jj workspace.
4. Move a top-level issue to `in-progress` and confirm a bookmark is created.
5. Move a child issue to `in-progress` and confirm a sibling workspace is created and bootstrapped.
6. Move that child issue to `done` and confirm workspace cleanup runs.
