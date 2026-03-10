# post-create-automation

## What it teaches

- Post-hook automation with `issue:created`
- Writing comments from extensions
- Optional follow-up issue creation via config

## Hooks

- `issue:created`

## Config

`examples/post-create-automation/.fp/config.toml`:

```toml
[extensions.post-create-automation]
trigger_statuses = "backlog,todo"
welcome_comment = "Thanks for opening this issue. Add context, acceptance criteria, and links."
create_followup = "false"
followup_title_prefix = "Research: "
```

## Quick test

1. Copy `.fp/extensions/post-create-automation.ts` into your project.
2. Add the config block under your project `.fp/config.toml`.
3. Create a new issue in `backlog` or `todo` and confirm a comment is posted.
4. Set `create_followup = "true"`, create another issue, and confirm a child issue is created.
