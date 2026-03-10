# Extension Examples Catalog

This directory is a teaching ladder for FP extensions, from logging-only hooks to workflow automation.

| Example | Complexity | Hooks used | Side effects | Runtime assumptions |
|---|---|---|---|---|
| `hello-hooks` | Beginner | `issue:created`, `issue:status:changed`, `comment:created` | Logs only | Node-compatible APIs only |
| `status-transition-guard` | Beginner | `issue:status:changing` | Blocks invalid transitions | Node-compatible APIs only |
| `post-create-automation` | Beginner | `issue:created` | Creates comments, optional child issues | Node-compatible APIs only |
| `custom-properties` | Beginner | `registerProperty` | None (registration only) | Node-compatible APIs only |
| `quality-gate` | Intermediate | `issue:status:changing` | Runs local checks; blocks transition on failure | Requires `bun` commands in project |
| `backlog-researcher` | Intermediate | `issue:created` | Runs `claude`, posts comments | Requires `claude` CLI |
| `jj-workspace` | Advanced | `issue:status:changing`, `issue:status:changed` | Creates/deletes jj bookmarks and workspaces | Requires `jj`; install command available in workspace |
| `cursor-agent` | Advanced | `registerProperty`, `registerAction`, `secrets`, `notify` | Launches external agent, polls status, posts comments | Requires Cursor API key (desktop only) |

## Suggested learning order

1. `hello-hooks`
2. `status-transition-guard`
3. `post-create-automation`
4. `custom-properties`
5. `quality-gate`
6. `backlog-researcher`
7. `jj-workspace`
8. `cursor-agent`
