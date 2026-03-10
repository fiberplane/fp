# cursor-agent

## What it teaches

- Registering typed custom issue fields with Standard Schema validation
- Using OS keychain secrets for API credentials
- Registering UI actions for command palette and detail view
- Background polling with setInterval
- Rich agent prompt construction (issue context + AGENTS.md/.cursor rules)
- Toast notifications via fp.ui.notify

## APIs used

- `fp.issues.registerProperty` — typed property with display configuration
- `fp.secrets.get` — OS keychain credential access
- `fp.ui.registerAction` — command palette/contextual actions with custom icon names
- `fp.ui.notify` — toast notifications
- `fp.issues.update` — status transitions and field updates
- `fp.comments.create` — posting agent summaries
- `fp.config.get` — reading extension config

## Config

`examples/cursor-agent/.fp/config.toml`:

```toml
[extensions.cursor-agent]
api-key = "secret:api-key"
auto-complete = false
```

The `api-key` setting is stored in your OS keychain (macOS Keychain or Linux secret-tool), not in the TOML file. Set it via the Extensions settings screen in the desktop app.

## Quick test

1. Copy `.fp/extensions/cursor-agent.ts` into your project's `.fp/extensions/` directory.
2. Add the config block under your project `.fp/config.toml`.
3. Set your Cursor API key in the desktop app's Extensions settings.
4. Open an issue and run "Send to Cursor Agent" from the command palette (Cmd+K).
5. Watch the badge update as the agent progresses.
