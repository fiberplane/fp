use anyhow::{anyhow, bail, Context, Result};
use dialoguer::{theme, Confirm, FuzzySelect, Input, MultiSelect, Select};
use fiberplane::api_client::ApiClient;
use fiberplane::base64uuid::Base64Uuid;
use fiberplane::models::data_sources::DataSource;
use fiberplane::models::names::Name;
use fiberplane::models::notebooks::NotebookSearch;
use fiberplane::models::paging::Pagination;
use fiberplane::models::sorting::{NotebookSortFields, SortDirection};
use fiberplane::models::webhooks::{InvalidWebhookCategoryError, WebhookCategory};
use indicatif::ProgressBar;
use std::convert::TryInto;
use std::time::Duration;
use strum::IntoEnumIterator;

pub fn default_theme() -> impl theme::Theme {
    theme::SimpleTheme
}

/// Sluggify some text to a valid Name
///
/// Return None if the input cannot be transformed (i.e. it contains
/// only emojis or something)
pub fn sluggify_str(input: &str) -> Option<Name> {
    let candidate: String = input
        .chars()
        .flat_map(char::to_lowercase)
        .flat_map(|c| match c {
            lower if lower.is_ascii_lowercase() => Some(lower),
            punct_space
                if punct_space.is_ascii_punctuation() || punct_space.is_ascii_whitespace() =>
            {
                Some('-')
            }
            _ => None,
        })
        .collect();

    let trimmed = candidate[0..=63.min(candidate.len())].trim_matches('-');

    trimmed.parse().ok()
}

/// Get the value from either a CLI argument, interactive input, or from a
/// default value. If no value is provided by the user and there is no default
/// value, it will return None.
///
/// NOTE: If the user does not specifies a value through a cli argument, the
/// interactive input will always be shown. This is a limitation that we
/// currently not check if the invocation is interactive or not.
pub fn text_opt<P>(prompt: P, argument: Option<String>, default: Option<String>) -> Option<String>
where
    P: Into<String>,
{
    if argument.is_some() {
        return argument;
    }

    let input = match &default {
        Some(default) => Input::with_theme(&default_theme())
            .with_prompt(prompt)
            .allow_empty(true)
            .default(default.clone())
            .interact(),
        None => Input::with_theme(&default_theme())
            .with_prompt(prompt)
            .allow_empty(true)
            .interact(),
    };

    match input {
        Ok(input) => {
            if input.is_empty() {
                default
            } else {
                Some(input)
            }
        }
        // TODO: Properly check for the error instead of just returning the default value.
        Err(_) => default,
    }
}

/// Get the value from either a argument, interactive input, or from a default
/// value. If the user does not supply a value then this function will return an
/// error. Use `text_opt` if you want to allow a None value.
///
/// NOTE: If the user does not specifies a value through a cli argument, the
/// interactive input will always be shown. This is a limitation that we
/// currently not check if the invocation is interactive or not.
pub fn text_req<P>(prompt: P, argument: Option<String>, default: Option<String>) -> Result<String>
where
    P: Into<String>,
{
    match text_opt(prompt, argument, default) {
        Some(value) => Ok(value),
        None => bail!("No value provided"),
    }
}

/// Get the value from either a CLI argument, interactive input, or from a
/// default value. If no value is provided by the user and there is no default
/// value, it will return None.
///
/// NOTE: If the user does not specifies a value through a cli argument, the
/// interactive input will always be shown. This is a limitation that we
/// currently not check if the invocation is interactive or not.
pub fn name_opt<P>(prompt: P, argument: Option<Name>, default: Option<Name>) -> Option<Name>
where
    P: Into<String>,
{
    if argument.is_some() {
        return argument;
    }

    let input = match &default {
        Some(default) => Input::with_theme(&default_theme())
            .with_prompt(prompt)
            .allow_empty(true)
            .default(default.clone())
            .interact(),
        None => Input::with_theme(&default_theme())
            .with_prompt(prompt)
            .allow_empty(true)
            .interact(),
    };

    match input {
        Ok(input) => {
            if input.is_empty() {
                default
            } else {
                Some(input)
            }
        }
        // TODO: Properly check for the error instead of just returning the default value.
        Err(_) => default,
    }
}

/// Get the value from either a argument, interactive input, or from a default
/// value. If the user does not supply a value then this function will return an
/// error. Use `text_opt` if you want to allow a None value.
///
/// NOTE: If the user does not specifies a value through a cli argument, the
/// interactive input will always be shown. This is a limitation that we
/// currently not check if the invocation is interactive or not.
pub fn name_req<P>(prompt: P, argument: Option<Name>, default: Option<Name>) -> Result<Name>
where
    P: Into<String>,
{
    match name_opt(prompt, argument, default) {
        Some(value) => Ok(value),
        None => bail!("No value provided"),
    }
}

/// Get the value from either a CLI argument, interactive input, or from a
/// default value.
///
/// NOTE: If the user does not specify a value through a cli argument, the
/// interactive input will always be shown. This is a limitation that we
/// currently do not check if the invocation is interactive or not.
pub fn bool_req<P>(prompt: P, argument: Option<bool>, default: bool) -> bool
where
    P: Into<String>,
{
    if let Some(argument) = argument {
        return argument;
    }

    let default_selected = if default { 0 } else { 1 };

    let theme = default_theme();
    let select = Select::with_theme(&theme)
        .with_prompt(prompt)
        .item("Yes")
        .item("No")
        .default(default_selected);
    let input = select.interact();

    match input {
        Ok(0) => true,
        Ok(1) => false,
        _ => default,
    }
}

/// Get a notebook ID from either a CLI argument, or from a interactive picker.
///
/// It works exactly as [notebook_picker_with_prompt](), but has a generic, default
/// prompt.
pub async fn notebook_picker(
    client: &ApiClient,
    argument: Option<Base64Uuid>,
    workspace_id: Option<Base64Uuid>,
) -> Result<Base64Uuid> {
    notebook_picker_with_prompt("Notebook", client, argument, workspace_id).await
}

/// Get a notebook ID from either a CLI argument, or from a interactive picker.
///
/// If the user has not specified the notebook ID through a CLI argument then it
/// will retrieve recent notebooks using the notebook search endpoint, and allow
/// the user to select one.
///
/// This will also ask for the workspace ID if it is not passed in as an
/// argument. If multiple pickers require the workspace ID, it is recommended to
/// do this once and then pass it to the other pickers as an argument.
///
/// NOTE: This currently does not do any limiting of the result nor does it do
/// any sorting. It will allow client side filtering.
/// NOTE: If the user does not specifies a value through a cli argument, the
/// interactive input will always be shown. This is a limitation that we
/// currently not check if the invocation is interactive or not.
pub async fn notebook_picker_with_prompt(
    prompt: &str,
    client: &ApiClient,
    argument: Option<Base64Uuid>,
    workspace_id: Option<Base64Uuid>,
) -> Result<Base64Uuid> {
    // If the user provided an argument, use that. Otherwise show the picker.
    if let Some(id) = argument {
        return Ok(id);
    };

    // No argument was provided, so we need to know the workspace ID.
    let workspace_id = workspace_picker_with_prompt(
        &format!("Workspace (to pick {prompt})"),
        client,
        workspace_id,
    )
    .await?;

    let pb = ProgressBar::new_spinner();
    pb.set_message("Fetching recent notebooks");
    pb.enable_steady_tick(Duration::from_millis(100));

    let results = client
        .notebook_search(
            workspace_id,
            Some(NotebookSortFields::CreatedAt.into()),
            Some(SortDirection::Descending.into()), // show notebooks which have been created most recently first
            NotebookSearch::default(),
        )
        .await?;

    pb.finish_and_clear();

    if results.is_empty() {
        bail!("No notebook id provided and no notebooks found");
    }

    let display_items: Vec<_> = results
        .iter()
        .map(|notebook| format!("{} ({})", notebook.title, notebook.id))
        .collect();

    let selection = FuzzySelect::with_theme(&default_theme())
        .with_prompt(prompt)
        .items(&display_items)
        .default(0)
        .interact_opt()?;

    match selection {
        Some(selection) => Ok(results[selection].id),
        None => bail!("No notebook selected"),
    }
}

/// Get a (workspace id, template name) pair from either a CLI argument, or from a interactive picker.
///
/// If the user has not specified the template ID through a CLI argument then it
/// will retrieve recent templates using the template list endpoint, and allow
/// the user to select one.
///
/// NOTE: This currently does not do any limiting of the result. It will allow
/// client side filtering.
/// NOTE: If the user does not specifies a value through a cli argument, the
/// interactive input will always be shown. This is a limitation that we
/// currently not check if the invocation is interactive or not.
pub async fn template_picker(
    client: &ApiClient,
    template_name: Option<Name>,
    workspace_id: Option<Base64Uuid>,
) -> Result<(Base64Uuid, Name)> {
    // We need an workspace ID. If the user has not supplied it, show the
    // workspace picker.
    let workspace_id = workspace_picker(client, workspace_id).await?;

    // Now we know which workspace the user wants to use, so we can use the
    // template_name if the user supplied that, otherwise show the template
    // picker.
    if let Some(template_name) = template_name {
        return Ok((workspace_id, template_name));
    }

    let pb = ProgressBar::new_spinner();
    pb.set_message("Fetching templates");
    pb.enable_steady_tick(Duration::from_millis(100));

    let results = client
        .template_list(workspace_id, Some("updated_at"), Some("descending"))
        .await?;

    pb.finish_and_clear();

    if results.is_empty() {
        bail!("No templates found");
    }

    let display_items: Vec<_> = results
        .iter()
        .map(|template| template.name.to_string())
        .collect();

    let selection = FuzzySelect::with_theme(&default_theme())
        .with_prompt("Template")
        .items(&display_items)
        .default(0)
        .interact_opt()?;

    match selection {
        Some(selection) => Ok((
            workspace_id,
            results[selection]
                .name
                .parse()
                .context("invalid name was returned")?,
        )),
        None => bail!("No template selected"),
    }
}

/// Get a (workspace id, snippet name) pair from either a CLI argument, or from a interactive picker.
///
/// If the user has not specified the snippet ID through a CLI argument then it
/// will retrieve recent snippets using the snippet list endpoint, and allow
/// the user to select one.
///
/// NOTE: This currently does not do any limiting of the result. It will allow
/// client side filtering.
/// NOTE: If the user does not specifies a value through a cli argument, the
/// interactive input will always be shown. This is a limitation that we
/// currently not check if the invocation is interactive or not.
pub async fn snippet_picker(
    client: &ApiClient,
    snippet_name: Option<Name>,
    workspace_id: Option<Base64Uuid>,
) -> Result<(Base64Uuid, Name)> {
    // If the user provided an argument _and_ the workspace, use that. Otherwise show the picker.
    if let (Some(workspace), Some(name)) = (workspace_id, snippet_name) {
        return Ok((workspace, name));
    };

    // No argument was provided, so we need to know the workspace ID in order to query
    // the snippet name.
    let workspace_id =
        workspace_picker_with_prompt("Workspace of the snippet", client, workspace_id).await?;

    let pb = ProgressBar::new_spinner();
    pb.set_message("Fetching snippets");
    pb.enable_steady_tick(Duration::from_millis(100));

    let results = client
        .snippet_list(workspace_id, Some("updated_at"), Some("descending"))
        .await?;

    pb.finish_and_clear();

    if results.is_empty() {
        bail!("No snippets found");
    }

    let display_items: Vec<_> = results
        .iter()
        .map(|snippet| snippet.name.to_string())
        .collect();

    let selection = FuzzySelect::with_theme(&default_theme())
        .with_prompt("Snippet")
        .items(&display_items)
        .default(0)
        .interact_opt()?;

    match selection {
        Some(selection) => Ok((
            workspace_id,
            results[selection]
                .name
                .parse()
                .context("invalid name was returned")?,
        )),
        None => bail!("No snippet selected"),
    }
}

/// Get a trigger ID from either a CLI argument, or from a interactive picker.
///
/// If the user has not specified the trigger ID through a CLI argument then it
/// will retrieve recent triggers using the trigger list endpoint, and allow
/// the user to select one.
///
/// NOTE: This currently does not do any limiting of the result nor does it do
/// any sorting. It will allow client side filtering.
/// NOTE: If the user does not specifies a value through a cli argument, the
/// interactive input will always be shown. This is a limitation that we
/// currently not check if the invocation is interactive or not.
pub async fn trigger_picker(
    client: &ApiClient,
    argument: Option<Base64Uuid>,
    workspace_id: Option<Base64Uuid>,
) -> Result<Base64Uuid> {
    // If the user provided an argument, use that. Otherwise show the picker.
    if let Some(id) = argument {
        return Ok(id);
    };

    // No argument was provided, so we need to know the workspace ID.
    let workspace_id = workspace_picker(client, workspace_id).await?;

    let pb = ProgressBar::new_spinner();
    pb.set_message("Fetching triggers");
    pb.enable_steady_tick(Duration::from_millis(100));

    let results = client.trigger_list(workspace_id).await?;

    pb.finish_and_clear();

    if results.is_empty() {
        bail!("No triggers found");
    }

    let display_items: Vec<_> = results
        .iter()
        .map(|trigger| format!("{} ({})", trigger.title, trigger.id))
        .collect();

    let selection = FuzzySelect::with_theme(&default_theme())
        .with_prompt("Trigger")
        .items(&display_items)
        .default(0)
        .interact_opt()?;

    match selection {
        Some(selection) => Ok(results[selection].id),
        None => bail!("No trigger selected"),
    }
}

/// Get a proxy ID from either a CLI argument, or from a interactive picker.
///
/// If the user has not specified the proxy ID through a CLI argument then it
/// will retrieve recent proxies using the proxy list endpoint, and allow
/// the user to select one.
///
/// NOTE: This currently does not do any limiting of the result nor does it do
/// any sorting. It will allow client side filtering.
/// NOTE: If the user does not specifies a value through a cli argument, the
/// interactive input will always be shown. This is a limitation that we
/// currently not check if the invocation is interactive or not.
pub async fn proxy_picker(
    client: &ApiClient,
    workspace_id: Option<Base64Uuid>,
    argument: Option<Name>,
) -> Result<Name> {
    // If the user provided an argument, use that. Otherwise show the picker.
    if let Some(name) = argument {
        return Ok(name);
    };

    // No argument was provided, so we need to know the workspace ID.
    let workspace_id = workspace_picker(client, workspace_id).await?;

    let pb = ProgressBar::new_spinner();
    pb.set_message("Fetching daemons");
    pb.enable_steady_tick(Duration::from_millis(100));

    let results = client.proxy_list(workspace_id).await?;

    pb.finish_and_clear();

    if results.is_empty() {
        bail!("No daemons found");
    }

    let display_items: Vec<_> = results.iter().map(|proxy| &proxy.name).collect();

    let selection = FuzzySelect::with_theme(&default_theme())
        .with_prompt("Daemon")
        .items(&display_items)
        .default(0)
        .interact_opt()?;

    match selection {
        Some(selection) => Ok(results[selection].name.clone()),
        None => bail!("No daemon selected"),
    }
}

/// Get a data source Name from either a CLI argument, or from a interactive picker.
///
/// If the user has not specified the data source name through a CLI argument then it
/// will retrieve recent data sources using the data source list endpoint, and allow
/// the user to select one.
///
/// NOTE: This currently does not do any limiting of the result nor does it do
/// any sorting. It will allow client side filtering.
/// NOTE: If the user does not specify a value through a cli argument, the
/// interactive input will always be shown. This is a limitation that we
/// currently not check if the invocation is interactive or not.
pub async fn data_source_picker(
    client: &ApiClient,
    workspace_id: Option<Base64Uuid>,
    argument: Option<Name>,
) -> Result<DataSource> {
    let workspace_id = workspace_picker(client, workspace_id).await?;

    if let Some(name) = argument {
        let data_source = client.data_source_get(workspace_id, &name).await?;
        return Ok(data_source);
    }

    let pb = ProgressBar::new_spinner();
    pb.set_message("Fetching data sources");
    pb.enable_steady_tick(Duration::from_millis(100));

    let mut results = client.data_source_list(workspace_id).await?;

    pb.finish_and_clear();

    if results.is_empty() {
        bail!("No data sources found");
    }

    let display_items: Vec<_> = results
        .iter()
        .map(|data_source| format!("{} ({})", &data_source.name, &data_source.provider_type))
        .collect();

    let selection = FuzzySelect::with_theme(&default_theme())
        .with_prompt("Data source")
        .items(&display_items)
        .default(0)
        .interact_opt()?;

    match selection {
        Some(selection) => Ok(results.remove(selection)),
        None => bail!("No data source selected"),
    }
}

/// Get a view name from either a CLI argument, or from a interactive picker.
///
/// If the user has not specified the view name through a CLI argument then it
/// will retrieve all views for the workspace using the views list endpoint, and allow
/// the user to select one.
///
/// NOTE: This currently does not do any limiting of the result nor does it do
/// any sorting. It will allow client side filtering.
/// NOTE: If the user does not specify a value through a cli argument, the
/// interactive input will always be shown. This is a limitation that we
/// currently not check if the invocation is interactive or not.
pub async fn view_picker(
    client: &ApiClient,
    workspace_id: Option<Base64Uuid>,
    argument: Option<Name>,
) -> Result<Name> {
    let workspace_id = workspace_picker(client, workspace_id).await?;

    if let Some(id) = argument {
        return Ok(id);
    }

    let pb = ProgressBar::new_spinner();
    pb.set_message("Fetching views");
    pb.enable_steady_tick(Duration::from_millis(100));

    let results = client
        .view_list(workspace_id, None, None, None, None)
        .await?;

    pb.finish_and_clear();

    if results.is_empty() {
        bail!("No views found");
    }

    let display_items: Vec<_> = results
        .iter()
        .map(|view| format!("{} ({})", view.display_name, view.id))
        .collect();

    let selection = FuzzySelect::with_theme(&default_theme())
        .with_prompt("View")
        .items(&display_items)
        .default(0)
        .interact_opt()?;

    match selection {
        Some(selection) => Ok(results[selection].name.clone()),
        None => bail!("No workspace selected"),
    }
}

/// Get a workspace ID from either a CLI argument, or from a interactive picker.
///
///
/// It works exactly as [workspace_picker_with_prompt](), but it uses a default,
/// generic prompt.
pub async fn workspace_picker(
    client: &ApiClient,
    argument: Option<Base64Uuid>,
) -> Result<Base64Uuid> {
    workspace_picker_with_prompt("Workspace", client, argument).await
}

/// Get a workspace ID from either a CLI argument, or from a interactive picker.
///
/// If the user has not specified the template ID through a CLI argument then it
/// will retrieve recent templates using the template list endpoint, and allow
/// the user to select one.
///
/// NOTE: This currently does not do any limiting of the result. It will allow
/// client side filtering.
/// NOTE: If the user does not specifies a value through a cli argument, the
/// interactive input will always be shown. This is a limitation that we
/// currently not check if the invocation is interactive or not.
pub async fn workspace_picker_with_prompt(
    prompt: &str,
    client: &ApiClient,
    argument: Option<Base64Uuid>,
) -> Result<Base64Uuid> {
    // If the user provided an argument, use that. Otherwise show the picker.
    if let Some(id) = argument {
        return Ok(id);
    };

    let pb = ProgressBar::new_spinner();
    pb.set_message("Fetching workspaces");
    pb.enable_steady_tick(Duration::from_millis(100));

    let results = client
        .workspace_list(Some("name"), Some("ascending"))
        .await?;

    pb.finish_and_clear();

    if results.is_empty() {
        bail!("No workspaces found");
    }

    let display_items: Vec<_> = results
        .iter()
        .map(|template| format!("{} ({})", template.name, template.id))
        .collect();

    let selection = FuzzySelect::with_theme(&default_theme())
        .with_prompt(prompt)
        .items(&display_items)
        .default(0)
        .interact_opt()?;

    match selection {
        Some(selection) => Ok(results[selection].id),
        None => bail!("No workspace selected"),
    }
}

/// Get a workspace user ID from either a CLI argument, or from a interactive picker.
///
/// If the user has not specified the workspace user ID through a CLI argument then it
/// will retrieve all users from that workspace using the workspace members list endpoint, and allow
/// the user to select one.
///
/// NOTE: This currently does not do any limiting of the result. It will allow
/// client side filtering.
/// NOTE: If the user does not specify a value through a cli argument, the
/// interactive input will always be shown. This is a limitation that we
/// currently not check if the invocation is interactive or not.
pub async fn workspace_user_picker(
    client: &ApiClient,
    workspace_id: &Base64Uuid,
    argument: Option<Base64Uuid>,
) -> Result<Base64Uuid> {
    // If the user provided an argument, use that. Otherwise show the picker.
    if let Some(id) = argument {
        return Ok(id);
    };

    let pb = ProgressBar::new_spinner();
    pb.set_message("Fetching workspace users");
    pb.enable_steady_tick(Duration::from_millis(100));

    let results = client
        .workspace_user_list(*workspace_id, Some("name"), Some("ascending"))
        .await?;

    pb.finish_and_clear();

    if results.is_empty() {
        bail!("No workspace users found");
    }

    let display_items: Vec<_> = results
        .iter()
        .map(|user| format!("{} ({})", user.name, user.id))
        .collect();

    let selection = FuzzySelect::with_theme(&default_theme())
        .with_prompt("Workspace Member")
        .items(&display_items)
        .default(0)
        .interact_opt()?;

    match selection {
        Some(selection) => Ok(results[selection].id),
        None => bail!("No workspace user selected"),
    }
}

/// Get a specific workspaces' webhook ID from either a CLI argument, or from an interactive picker.
///
/// If the user has not specified the webhook ID through a CLI argument then it
/// will retrieve all webhooks from that workspace using the workspace webhook list endpoint, and allow
/// the user to select one.
///
/// NOTE: This currently does not do any limiting of the result.
/// NOTE: If the user does not specify a value through a cli argument, the
/// interactive input will always be shown. This is a limitation that we
/// currently not check if the invocation is interactive or not.
pub async fn webhook_picker(
    client: &ApiClient,
    workspace_id: Base64Uuid,
    argument: Option<Base64Uuid>,
) -> Result<Base64Uuid> {
    // If the user provided an argument, use that. Otherwise, show the picker.
    if let Some(id) = argument {
        return Ok(id);
    };

    let pb = ProgressBar::new_spinner();
    pb.set_message("Fetching webhooks");
    pb.enable_steady_tick(Duration::from_millis(100));

    let max = Pagination::max();
    let results = client
        .webhook_list(workspace_id, Some(max.page as i32), Some(max.limit as i32))
        .await?;

    pb.finish_and_clear();

    if results.is_empty() {
        bail!("No webhooks found in the workspace");
    }

    let display_items: Vec<_> = results
        .iter()
        .map(|webhook| format!("{} ({})", webhook.endpoint, webhook.id))
        .collect();

    let selection = FuzzySelect::with_theme(&default_theme())
        .with_prompt("Webhook")
        .items(&display_items)
        .default(0)
        .interact_opt()?;

    match selection {
        Some(selection) => Ok(results[selection].id),
        None => bail!("No webhook selected"),
    }
}

/// Get a specific workspaces' webhook delivery ID from either a CLI argument, or from an interactive picker.
///
/// If the user has not specified the webhook delivery ID through a CLI argument then it
/// will retrieve all webhooks deliveries from that workspace using the workspace webhook delivery list endpoint, and allow
/// the user to select one.
///
/// NOTE: This currently does not do any limiting of the result.
/// NOTE: If the user does not specify a value through a cli argument, the
/// interactive input will always be shown. This is a limitation that we
/// currently not check if the invocation is interactive or not.
pub async fn webhook_delivery_picker(
    client: &ApiClient,
    workspace_id: Base64Uuid,
    webhook_id: Base64Uuid,
    argument: Option<Base64Uuid>,
) -> Result<Base64Uuid> {
    // If the user provided an argument, use that. Otherwise, show the picker.
    if let Some(id) = argument {
        return Ok(id);
    };

    let pb = ProgressBar::new_spinner();
    pb.set_message("Fetching webhook deliveries");
    pb.enable_steady_tick(Duration::from_millis(100));

    let max = Pagination::max();
    let results = client
        .webhook_delivery_list(
            workspace_id,
            webhook_id,
            Some(max.page as i32),
            Some(max.limit as i32),
        )
        .await?;

    pb.finish_and_clear();

    if results.is_empty() {
        bail!("No webhook deliveries found for webhook");
    }

    let display_items: Vec<_> = results
        .iter()
        .map(|delivery| format!("{} ({})", delivery.id, delivery.event))
        .collect();

    let selection = FuzzySelect::with_theme(&default_theme())
        .with_prompt("Webhook Delivery")
        .items(&display_items)
        .default(0)
        .interact_opt()?;

    match selection {
        Some(selection) => Ok(results[selection].id),
        None => bail!("No webhook delivery selected"),
    }
}

pub fn webhook_category_picker(
    input: Option<Vec<WebhookCategory>>,
) -> Result<Vec<WebhookCategory>> {
    match input {
        Some(categories) => Ok(categories),
        None => {
            let mut categories = Vec::new();

            for category in WebhookCategory::iter() {
                categories.push(category.to_string());
            }

            let items = MultiSelect::new()
                .with_prompt("Categories")
                .items(&categories)
                .interact()?;

            let categories: Result<Vec<WebhookCategory>, InvalidWebhookCategoryError> = items
                .into_iter()
                .map(|index| index as i16) // only i16 has a From impl
                .map(|index| index.try_into())
                .collect();

            categories.map_err(|err| anyhow!(err))
        }
    }
}

/// Get a (workspace id, front matter collection name) pair from either a CLI argument, or from a interactive picker.
///
/// If the user has not specified the front matter collection through a CLI argument then it
/// will retrieve recent snippets using the snippet list endpoint, and allow
/// the user to select one.
///
/// NOTE: This currently does not do any limiting of the result. It will allow
/// client side filtering.
/// NOTE: If the user does not specifies a value through a cli argument, the
/// interactive input will always be shown. This is a limitation that we
/// currently not check if the invocation is interactive or not.
pub async fn front_matter_collection_picker(
    client: &ApiClient,
    workspace_id: Option<Base64Uuid>,
    front_matter_collection_name: Option<Name>,
) -> Result<(Base64Uuid, Name)> {
    // If the user provided an argument _and_ the workspace, use that. Otherwise show the picker.
    if let (Some(workspace), Some(name)) = (workspace_id, front_matter_collection_name) {
        return Ok((workspace, name));
    };

    // No argument was provided, so we need to know the workspace ID in order to query
    // the snippet name.
    let workspace_id = workspace_picker_with_prompt(
        "Workspace of the front matter collection",
        client,
        workspace_id,
    )
    .await?;

    let pb = ProgressBar::new_spinner();
    pb.set_message("Fetching front matter collections");
    pb.enable_steady_tick(Duration::from_millis(100));

    let results = client
        .workspace_front_matter_schema_get(workspace_id)
        .await?;

    pb.finish_and_clear();

    if results.is_empty() {
        bail!("No front matter collection found");
    }

    let display_items: Vec<_> = results
        .iter()
        .map(|(name, _schema)| name.to_string())
        .collect();

    let selection = FuzzySelect::with_theme(&default_theme())
        .with_prompt("Front matter collection")
        .items(&display_items)
        .default(0)
        .interact_opt()?;

    match selection {
        Some(selection) => Ok((
            workspace_id,
            display_items[selection]
                .parse()
                .context("invalid name was returned")?,
        )),
        None => bail!("No front matter collection selected"),
    }
}

pub fn confirm(prompt: impl Into<String>) -> Result<bool> {
    Confirm::new()
        .with_prompt(prompt)
        .interact()
        .map_err(|err| anyhow!(err))
}

/// Interactively select one of the given items
pub fn select_item<P, T>(prompt: P, items: &[T], default: Option<usize>) -> Result<usize>
where
    P: Into<String>,
    T: ToString,
{
    FuzzySelect::with_theme(&default_theme())
        .with_prompt(prompt)
        .items(items)
        .default(default.unwrap_or(0))
        .interact()
        .map_err(|err| err.into())
}
