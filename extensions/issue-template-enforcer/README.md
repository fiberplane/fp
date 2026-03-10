# issue-template-enforcer

## What it teaches

- Pre-hook validation on issue *creation* (`issue:creating`)
- Returning structured validation errors with helpful guidance
- Configurable skip conditions for low-friction workflows
- Regex-based markdown section detection

## Hooks

- `issue:creating`

## Config

`examples/issue-template-enforcer/.fp/config.toml`:

```toml
[extensions.issue-template-enforcer]
required_sections = "Context,Acceptance Criteria"
min_description_length = "30"
skip_statuses = ""
```

- **required_sections**: Comma-separated list of markdown heading names that must appear in the description (case-insensitive, any heading level).
- **min_description_length**: Minimum character count for the description. Issues shorter than this are rejected with a template skeleton.
- **skip_statuses**: Comma-separated list of statuses that bypass validation entirely. Useful if your project has custom statuses for quick dumps (e.g. a `backlog` status).

## Quick test

1. Copy `.fp/extensions/issue-template-enforcer.ts` into your project.
2. Add the config block under your project `.fp/config.toml`.
3. Try creating an issue in `todo` with no description — confirm it is rejected with a template skeleton.
4. Create an issue in `todo` with both `## Context` and `## Acceptance Criteria` sections — confirm it passes.
5. Optionally, set `skip_statuses = "todo"` and confirm `todo` issues bypass validation.
