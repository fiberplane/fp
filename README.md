<p align="center">
  <img src="assets/fp-icon.svg" alt="fp" width="80" height="80" />
</p>

<h3 align="center">fp</h3>

<p align="center">
  Agent-native issue tracking for ambitious Claude Code users.
  <br />
  <a href="https://fp.dev"><strong>fp.dev</strong></a> · <a href="https://fp.dev/docs">Docs</a>
</p>

---

fp is a CLI-first issue tracker designed for AI agents. It lives in your terminal, stores everything locally, and gives agents the focused context they need to ship code across sessions.

This is the public repository for fp. The source code is private (for now). This repo is for:

- **Filing issues** — bug reports, feature requests, and feedback
- **Extension examples** — reference implementations for fp's extension system
- **Community discussion** — via GitHub Issues and Discussions

## Get started

```bash
curl -fsSL https://setup.fp.dev/install.sh | sh -s
cd your-project
fp init
```

See the [docs](https://fp.dev/docs) for full setup and usage.

## Extensions

fp supports extensions that hook into issue lifecycle events. The [`extensions/`](extensions/) directory contains example extensions ranging from beginner to advanced:

| Example | Complexity | What it does |
|---|---|---|
| [`hello-hooks`](extensions/hello-hooks) | Beginner | Logs issue events — the smallest useful extension |
| [`status-transition-guard`](extensions/status-transition-guard) | Beginner | Blocks invalid status transitions |
| [`post-create-automation`](extensions/post-create-automation) | Beginner | Posts welcome comments, creates follow-up issues |
| [`custom-properties`](extensions/custom-properties) | Beginner | Registers custom issue properties (select, text, etc.) |
| [`quality-gate`](extensions/quality-gate) | Intermediate | Runs tests/lint before allowing "done" transition |
| [`backlog-researcher`](extensions/backlog-researcher) | Intermediate | Spawns Claude to research new issues |
| [`jj-workspace`](extensions/jj-workspace) | Advanced | Manages jj workspaces tied to issue lifecycle |
| [`cursor-agent`](extensions/cursor-agent) | Advanced | Dispatches issues to a Cursor agent with polling |

Start with `hello-hooks` and work your way up. Each example has its own README.

## Reporting bugs

Please [open an issue](../../issues/new) with steps to reproduce.

## License

The extension examples in this repository are released under the [MIT License](LICENSE). fp itself is proprietary software — see [fp.dev](https://fp.dev) for details.
