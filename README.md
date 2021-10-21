# Fiberplane CLI ⌨️

A command line tool for interacting with Fiberplane.

## Usage

### Login

Before running most commands, you'll need to authorize the CLI to act on your behalf.
```shell
fp login
```
This will open a web browser and take you through the Google OAuth flow.

### Add a Proxy

The [Proxy](https://github.com/fiberplane/proxy) enables Fiberplane to connect to your organization's data sources (e.g. Prometheus) without exposing them to the public internet.

```shell
fp proxy add my-first-proxy
```
This will register a Proxy with the name `my-first-proxy` and return the Proxy Auth Token you will run the Proxy instance with.

You can use any name for your proxy or proxies. Organizations may have multiple proxies for different Kubernetes clusters or environments (production, staging, etc). We would recommend giving each one a name that corresponds to the environment it will be running in on your infrastructure.

### Managing Proxies

The CLI offers other commands to help view and manage your organization's proxies:

(Note that `fp proxy` and `fp proxies` can be used interchangeably).

- `fp proxies list` - shows your proxies' names, IDs, and connection statuses
- `fp proxies data-sources` - shows the data sources exposed by all of your proxies
- `fp proxies inspect <proxy_id>` - shows detailed information about a specific proxy
- `fp proxy remove <proxy_id>` - remove the given proxy
