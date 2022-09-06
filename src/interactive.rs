use anyhow::anyhow;
use anyhow::Context;
use anyhow::Result;
use base64uuid::Base64Uuid;
use dialoguer::{theme, FuzzySelect, Input, Select};
use fp_api_client::apis::default_api::proxy_list;
use fp_api_client::apis::default_api::template_list;
use fp_api_client::apis::default_api::trigger_list;
use fp_api_client::apis::{configuration::Configuration, default_api::notebook_search};
use fp_api_client::models::NotebookSearch;
use indicatif::ProgressBar;

fn default_theme() -> impl theme::Theme {
    theme::SimpleTheme
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

/// Get the value from either a CLI argument, interactive input, or from a
/// default value. If no value is provided by the user and there is no default
/// value, it will return None.
///
/// NOTE: If the user does not specifies a value through a cli argument, the
/// interactive input will always be shown. This is a limitation that we
/// currently not check if the invocation is interactive or not.
pub fn bool_opt<P>(prompt: P, argument: Option<bool>, default: Option<bool>) -> Option<bool>
where
    P: Into<String>,
{
    if argument.is_some() {
        return argument;
    }
    let theme = default_theme();
    let mut select = Select::with_theme(&theme);
    select.with_prompt(prompt).item("Yes").item("No");
    match default {
        Some(true) => {
            select.default(0);
        }
        Some(false) => {
            select.default(1);
        }
        _ => {}
    }
    let input = select.interact();

    match input {
        Ok(input) => match input {
            0 => Some(true),
            1 => Some(false),
            _ => default,
        },
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
        None => Err(anyhow!("No value provided")),
    }
}

/// Get a notebook ID from either a CLI argument, or from a interactive picker.
///
/// If the user has not specified the notebook ID through a CLI argument then it
/// will retrieve recent notebooks using the notebook search endpoint, and allow
/// the user to select one.
///
/// NOTE: This currently does not do any limiting of the result nor does it do
/// any sorting. It will allow client side filtering.
/// NOTE: If the user does not specifies a value through a cli argument, the
/// interactive input will always be shown. This is a limitation that we
/// currently not check if the invocation is interactive or not.
pub async fn notebook_picker(
    config: &Configuration,
    argument: Option<Base64Uuid>,
) -> Result<Base64Uuid> {
    // If the user provided an argument, use that. Otherwise show the picker.
    if let Some(id) = argument {
        return Ok(id);
    };

    let pb = ProgressBar::new_spinner();
    pb.set_message("Fetching recent notebooks");
    pb.enable_steady_tick(100);

    let results = notebook_search(config, NotebookSearch { labels: None }).await?;

    pb.finish_and_clear();

    if results.is_empty() {
        return Err(anyhow!("No notebook id provided and no notebooks found"));
    }

    let display_items: Vec<_> = results
        .iter()
        .map(|notebook| format!("{} ({})", notebook.title, notebook.id))
        .collect();

    let selection = FuzzySelect::with_theme(&default_theme())
        .with_prompt("Notebook")
        .items(&display_items)
        .default(0)
        .interact_opt()?;

    match selection {
        Some(selection) => {
            Ok(Base64Uuid::parse_str(&results[selection].id).context("invalid id was returned")?)
        }
        None => Err(anyhow!("No notebook selected")),
    }
}

/// Get a template ID from either a CLI argument, or from a interactive picker.
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
    config: &Configuration,
    argument: Option<Base64Uuid>,
) -> Result<Base64Uuid> {
    // If the user provided an argument, use that. Otherwise show the picker.
    if let Some(id) = argument {
        return Ok(id);
    };

    let pb = ProgressBar::new_spinner();
    pb.set_message("Fetching templates");
    pb.enable_steady_tick(100);

    let results = template_list(config, Some("updated_at"), Some("descending")).await?;

    pb.finish_and_clear();

    if results.is_empty() {
        return Err(anyhow!("No templates found"));
    }

    let display_items: Vec<_> = results
        .iter()
        .map(|template| format!("{} ({})", template.title, template.id))
        .collect();

    let selection = FuzzySelect::with_theme(&default_theme())
        .with_prompt("Template")
        .items(&display_items)
        .default(0)
        .interact_opt()?;

    match selection {
        Some(selection) => {
            Ok(Base64Uuid::parse_str(&results[selection].id).context("invalid id was returned")?)
        }
        None => Err(anyhow!("No template selected")),
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
    config: &Configuration,
    argument: Option<Base64Uuid>,
) -> Result<Base64Uuid> {
    // If the user provided an argument, use that. Otherwise show the picker.
    if let Some(id) = argument {
        return Ok(id);
    };

    let pb = ProgressBar::new_spinner();
    pb.set_message("Fetching triggers");
    pb.enable_steady_tick(100);

    let results = trigger_list(config).await?;

    pb.finish_and_clear();

    if results.is_empty() {
        return Err(anyhow!("No triggers found"));
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
        Some(selection) => {
            Ok(Base64Uuid::parse_str(&results[selection].id).context("invalid id was returned")?)
        }
        None => Err(anyhow!("No trigger selected")),
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
    config: &Configuration,
    argument: Option<Base64Uuid>,
) -> Result<Base64Uuid> {
    // If the user provided an argument, use that. Otherwise show the picker.
    if let Some(id) = argument {
        return Ok(id);
    };

    let pb = ProgressBar::new_spinner();
    pb.set_message("Fetching proxies");
    pb.enable_steady_tick(100);

    let results = proxy_list(config).await?;

    pb.finish_and_clear();

    if results.is_empty() {
        return Err(anyhow!("No proxies found"));
    }

    let display_items: Vec<_> = results
        .iter()
        .map(|trigger| format!("{} ({})", trigger.name, trigger.id))
        .collect();

    let selection = FuzzySelect::with_theme(&default_theme())
        .with_prompt("Proxy")
        .items(&display_items)
        .default(0)
        .interact_opt()?;

    match selection {
        Some(selection) => {
            Ok(Base64Uuid::parse_str(&results[selection].id).context("invalid id was returned")?)
        }
        None => Err(anyhow!("No proxy selected")),
    }
}
