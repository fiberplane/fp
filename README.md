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
- `fp proxies delete <proxy_id>` - delete the given proxy

### Templates

[Templates](https://github.com/fiberplane/templates) enable you to programmatically generate Fiberplane notebooks to run repeatable workflows.

You can browse our [example templates](https://github.com/fiberplane/templates/tree/main/examples) to see templates for use cases such as incident response and root cause analyses.

#### Creating Templates

Generate a template from an existing notebook with the `convert` command:

```shell
fp templates convert https://studio.fiberplane.com/notebook/My-Notebook-<NOTEBOOK_ID_HERE>
```

Alternatively, you can create a blank template with the `init` command:

```shell
fp templates init
```

See the [template API documentation](https://github.com/fiberplane/templates/blob/main/docs/template_api.md) for all of the methods available in the template library.

#### Using Templates to Create Notebooks

You can manually create a notebook from a template using the `expand` command:
```shell
fp templates expand <TEMPLATE_ID>
fp templates expand ./path/to/template.jsonnet
```

##### Passing Template Arguments

Most templates export a top-level function so that certain notebook details can be filled in at the time the notebook is created from the template.

You can pass function arguments via the CLI as simple key-value pairs: `fp templates expand <TEMPLATE_ID> arg1=value1,arg2=value2` or as a JSON object: `fp templates expand <TEMPLATE_ID> {"arg1":"value1","arg2":"value2"}`.

### Triggers

Triggers enable you to create Fiberplane Notebooks from [Templates](#templates) via an API call. This can be used to automatically create notebooks from alerts.

#### Creating Triggers

You can create a Trigger from a local template:

```shell
fp triggers create --template-id <TEMPLATE_ID>
```

This command returns the trigger URL used to invoke the trigger (see the next section).

#### Invoking Triggers

Normally, Triggers are invoked with HTTP POST requests to `https://studio.fiberplane.com/api/triggers/:id/:secret_key`. The Trigger's URL is printed when it is created via the CLI.

The CLI can be used to test out a trigger:
```shell
fp triggers invoke <TRIGGER_ID> <SECRET_KEY> arg1=value1,arg2=value2
```

#### Managing Triggers

The CLI also supports the following operations for your (organization's) triggers:
- `fp triggers list`
- `fp triggers get <id>`
- `fp triggers delete <id>`

### Notebooks

The CLI allows for management for management of your notebooks. Currently the
following commands are supported:

- `fp notebooks add`
- `fp notebooks get <id>`

#### Creating a new notebook

The `fp` cli is able to create a notebook. You can specify a couple of
parameters through arguments and then the cli will create a notebook for you.

```shell
fp notebooks add --title "test" --label key=value
```

#### Retrieving a notebook

It is also possible to retrieve the notebook and display it as JSON.

```shell
fp notebooks get <notebook_id>
```
