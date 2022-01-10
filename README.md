# Fiberplane CLI ⌨️

A command line tool for interacting with Fiberplane.

## Usage

### Login

Before running most commands, you'll need to authorize the CLI to act on your behalf.
```shell
fp login
```
This will open a web browser and take you through the Google OAuth flow.

### Proxies

The [Proxy](https://github.com/fiberplane/proxy) enables Fiberplane to connect to your organization's data sources (e.g. Prometheus) without exposing them to the public internet.

#### Add a Proxy

```shell
fp proxy add my-first-proxy
```
This will register a Proxy with the name `my-first-proxy` and return the Proxy Auth Token you will run the Proxy instance with.

You can use any name for your proxy or proxies. Organizations may have multiple proxies for different Kubernetes clusters or environments (production, staging, etc). We would recommend giving each one a name that corresponds to the environment it will be running in on your infrastructure.

#### Managing Proxies

The CLI offers other commands to help view and manage your organization's proxies:

(Note that `fp proxy` and `fp proxies` can be used interchangeably).

- `fp proxies list` - shows your proxies' names, IDs, and connection statuses
- `fp proxies data-sources` - shows the data sources exposed by all of your proxies
- `fp proxies inspect <proxy_id>` - shows detailed information about a specific proxy
- `fp proxy remove <proxy_id>` - remove the given proxy

### Templates

[Templates](https://github.com/fiberplane/templates) enable you to programmatically generate Fiberplane notebooks to run repeatable workflows.

#### Creating Templates

Generate a template from an existing notebook with the `convert` command:

```shell
fp templates convert https://fiberplane.com/notebook/My-Notebook-<NOTEBOOK_ID_HERE> --out template.jsonnet
```

Alternatively, you can create a blank template with the `init` command:

```shell
fp templates init
```

See the [template API documentation](https://github.com/fiberplane/templates/blob/main/docs/template_api.md) for all of the methods available in the template library.

#### Creating Notebooks from Templates

Create a notebook from a template with the `expand` command:
```shell
fp templates expand --create-notebook ./path/to/template.jsonnet
```

Most templates export a top-level function so that certain notebook details can be filled in at the time the notebook is created from the template.

You can pass function arguments via the CLI with `--arg name=value`. For example, if a template exports the function `function(title, service)`, you could pass `--arg "title=My Notebook" --arg service=api`).

In most cases, you will want to create a [Trigger](#triggers) so that notebooks can be created via an API call instead of manually creating them via the CLI.

### Triggers

Triggers enable you to create Fiberplane Notebooks from [Templates](#templates) via an API call. This can be used to automatically create notebooks from alerts.

#### Creating Triggers

You can create a Trigger from a local template:

```shell
fp triggers create ./path/to/template.jsonnet
```

Or from a remotely hosted template:
```shell
fp triggers create https://example.com/template.jsonnet
```

If you pass a template URL, the template will be fetched each time a notebook is created (this is useful in case you want to update your templates).

#### Invoking Triggers

Normally, Triggers are invoked with HTTP POST requests to `https://fiberplane.com/api/triggers/:id/webook`. The Trigger's webhook URL is printed when it is created via the CLI.

The CLI can be used to test out a trigger:
```shell
fp triggers invoke --arg name=value https://fiberplane.com/api/triggers/<TRIGGER_ID_HERE>
```

#### Managing Triggers

The CLI also supports the following operations for your (organization's) triggers:
- `fp triggers list`
- `fp triggers get <id>`
- `fp triggers delete <id>`
