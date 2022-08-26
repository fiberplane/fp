use crate::config::api_client_configuration;
use crate::output::{output_details, output_json, GenericKeyValue};
use anyhow::Result;
use clap::{ArgEnum, Parser};
use fp_api_client::apis::default_api::profile_get;
use fp_api_client::models::User;
use std::path::PathBuf;
use url::Url;

#[derive(Parser)]
pub struct Arguments {
    #[clap(subcommand)]
    sub_command: SubCommand,
}

#[derive(Parser)]
enum SubCommand {
    /// Get the profile of the current user
    Profile(GetArgs),
}

#[derive(ArgEnum, Clone)]
enum ProfileOutput {
    /// Output the details as a table
    Table,

    /// Output the details as JSON
    Json,
}

#[derive(Parser)]
struct GetArgs {
    /// Output of the template
    #[clap(long, short, default_value = "table", arg_enum)]
    output: ProfileOutput,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

pub async fn handle_command(args: Arguments) -> Result<()> {
    match args.sub_command {
        SubCommand::Profile(args) => handle_get_profile_command(args).await,
    }
}

async fn handle_get_profile_command(args: GetArgs) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;
    let user = profile_get(&config).await?;
    match args.output {
        ProfileOutput::Table => output_details(GenericKeyValue::from_user(user)),
        ProfileOutput::Json => output_json(&user),
    }?;
    Ok(())
}

impl GenericKeyValue {
    fn from_user(user: User) -> Vec<GenericKeyValue> {
        vec![
            GenericKeyValue::new("Name:", user.name),
            GenericKeyValue::new("ID:", user.id),
            GenericKeyValue::new("Email:", user.email.unwrap_or_default()),
        ]
    }
}
