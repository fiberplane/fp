use crate::config::api_client_configuration;
use crate::output::{output_details, output_json, GenericKeyValue};
use anyhow::Result;
use clap::{Parser, ValueEnum};
use fiberplane::api_client::profile_get;
use fiberplane::models::users::Profile;
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

#[derive(ValueEnum, Clone)]
enum ProfileOutput {
    /// Output the details as a table
    Table,

    /// Output the details as JSON
    Json,
}

#[derive(Parser)]
struct GetArgs {
    /// Output of the template
    #[clap(long, short, default_value = "table", value_enum)]
    output: ProfileOutput,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    profile: Option<String>,
}

pub async fn handle_command(args: Arguments) -> Result<()> {
    match args.sub_command {
        SubCommand::Profile(args) => handle_get_profile_command(args).await,
    }
}

async fn handle_get_profile_command(args: GetArgs) -> Result<()> {
    let client = api_client_configuration(args.profile.as_deref()).await?;
    let profile = profile_get(&client).await?;

    match args.output {
        ProfileOutput::Table => output_details(GenericKeyValue::from_profile(profile)),
        ProfileOutput::Json => output_json(&profile),
    }
}

impl GenericKeyValue {
    fn from_profile(user: Profile) -> Vec<GenericKeyValue> {
        vec![
            GenericKeyValue::new("Name:", user.name),
            GenericKeyValue::new("ID:", user.id),
            GenericKeyValue::new("Email:", user.email),
        ]
    }
}
