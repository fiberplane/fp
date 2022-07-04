use anyhow::anyhow;
use anyhow::Context;
use anyhow::Result;
use base64uuid::Base64Uuid;
use dialoguer::{theme, FuzzySelect, Input};
use fp_api_client::apis::default_api::proxy_list;
use fp_api_client::apis::default_api::template_list;
use fp_api_client::apis::default_api::trigger_list;
use fp_api_client::apis::{configuration::Configuration, default_api::notebook_search};
use fp_api_client::models::NotebookSearch;
use indicatif::ProgressBar;

fn default_theme() -> impl theme::Theme {
    theme::SimpleTheme
}

/// Get the value from either a argument, interactive input, or from a default
/// value.
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
/// value.
pub fn text_req<P>(prompt: P, argument: Option<String>, default: Option<String>) -> Result<String>
where
    P: Into<String>,
{
    match text_opt(prompt, argument, default) {
        Some(value) => Ok(value),
        None => Err(anyhow!("No value provided")),
    }
}

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

    let results = notebook_search(&config, NotebookSearch { labels: None }).await?;

    pb.finish_and_clear();

    if results.len() == 0 {
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

    let results = template_list(&config, Some("title"), Some("ascending")).await?;

    pb.finish_and_clear();

    if results.len() == 0 {
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

    let results = trigger_list(&config).await?;

    pb.finish_and_clear();

    if results.len() == 0 {
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

    let results = proxy_list(&config).await?;

    pb.finish_and_clear();

    if results.len() == 0 {
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
