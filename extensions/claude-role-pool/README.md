# claude-role-pool

Orchestrates a pool of Docker-based Claude Code sandboxes, one per role
(`planner`, `implementer`, `reviewer`), and dispatches issues to them based on
a custom `agent` property on each issue.

## What it teaches

- Managing long-lived external resources (Docker sandboxes) from an extension
- Registering typed custom properties with `select` and `multiselect` displays
- Combining `issue:created` / `issue:updated` hooks with command-palette actions
- Periodic reconciliation with `setInterval` to keep external state in sync
- Using `fp.config.get` to read project-level catalogs (available repositories)
- Detecting sandbox vs. host runtime and disabling orchestration inside sandboxes

## APIs used

- `fp.issues.registerProperty` — typed `agent` and `repositories` properties
- `fp.ui.registerAction` — refresh and per-role dispatch actions
- `fp.ui.notify` — toast notifications
- `fp.on("issue:created" | "issue:updated")` — auto-dispatch on property changes
- `fp.issues.update` — write back the selected role
- `fp.config.get` — `template` and `repository_catalog`
- `fp.log` — structured logging

## Hooks

- `issue:created`
- `issue:updated`

## Runtime requirements

- Desktop runtime (the extension no-ops on other runtimes)
- `sbx` CLI on `PATH` (Docker sandbox manager)
- A base template image, default `docker/sandbox-templates:claude-code-docker`
- `fp` CLI reachable from inside the sandbox at `host.docker.internal:7878`

## Config

`.fp/config.toml`:

```toml
[extensions.claude-role-pool]
template = "docker/sandbox-templates:claude-code-docker"

repository_catalog = [
  { id = "nocturne", label = "nocturne", url = "git@github.com:fiberplane/nocturne.git" },
  { id = "fp",       label = "fp",       url = "git@github.com:fiberplane/fp.git" },
]
```

Alternate key `repositories_catalog` is also accepted.

## Quick test

1. Copy `.fp/extensions/claude-role-pool.ts` into your project's `.fp/extensions/` directory.
2. Add the config block under your project `.fp/config.toml`.
3. Ensure the `sbx` CLI and the base template image are available locally.
4. Run the fp desktop app — the pool reconciles on startup and every 5 minutes.
5. Open an issue, set its `Agent` property to `planner`/`implementer`/`reviewer`,
   and optionally select one or more `Repositories`. The matching sandbox is
   provisioned and the issue is dispatched.
6. Or invoke `Send to Planner/Implementer/Reviewer Sandbox` from the command palette.
