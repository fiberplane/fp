use crate::config::api_client_configuration;
use crate::output::{output_details, output_json, output_list, GenericKeyValue};
use anyhow::Result;
use clap::{Parser, ValueEnum};
use cli_table::Table;
use fiberplane::api_client::{token_create, token_delete, token_list};
use fiberplane::base64uuid::Base64Uuid;
use fiberplane::models::sorting::{SortDirection, TokenListSortFields};
use fiberplane::models::tokens::{NewToken, Token, TokenSummary};
use time::format_description::well_known::Rfc3339;
use tracing::info;
use url::Url;

#[derive(Parser)]
pub struct Arguments {
    #[clap(subcommand)]
    sub_command: SubCommand,
}

pub async fn handle_command(args: Arguments) -> Result<()> {
    use SubCommand::*;
    match args.sub_command {
        Create(args) => handle_token_create_command(args).await,
        List(args) => handle_token_list_command(args).await,
        Delete(args) => handle_token_delete_command(args).await,
    }
}

#[derive(Parser)]
enum SubCommand {
    /// Create a token
    #[clap(alias = "add")]
    Create(CreateArguments),

    /// Lists all tokens
    List(ListArguments),

    /// Deletes a token
    #[clap(aliases = &["remove", "rm"])]
    Delete(DeleteArguments),
}

#[derive(ValueEnum, Clone)]
enum TokenCreateOutput {
    /// Output the details as a table
    Table,

    /// Output the details as JSON
    Json,

    /// Output only the token
    Token,
}

#[derive(ValueEnum, Clone)]
enum TokenListOutput {
    /// Output the details as a table
    Table,

    /// Output the details as JSON
    Json,
}

#[derive(Parser)]
struct CreateArguments {
    /// Name of the token
    #[clap(long)]
    name: String,

    /// Output of the token
    #[clap(long, short, default_value = "table", value_enum)]
    output: TokenCreateOutput,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    profile: Option<String>,
}

#[derive(Parser)]
pub struct ListArguments {
    /// Output of the token
    #[clap(long, short, default_value = "table", value_enum)]
    output: TokenListOutput,

    /// Sort the result according to the following field
    #[clap(long, value_enum)]
    sort_by: Option<TokenListSortFields>,

    /// Sort the result in the following direction
    #[clap(long, value_enum)]
    sort_direction: Option<SortDirection>,

    /// Page to display
    #[clap(long)]
    page: Option<i32>,

    /// Amount of events to display per page
    #[clap(long)]
    limit: Option<i32>,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    profile: Option<String>,
}

#[derive(Parser)]
pub struct DeleteArguments {
    /// ID of the token that should be deleted
    id: Base64Uuid,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    profile: Option<String>,
}

async fn handle_token_create_command(args: CreateArguments) -> Result<()> {
    let client = api_client_configuration(args.profile.as_deref()).await?;

    let token = token_create(&client, NewToken::new(args.name)).await?;

    if !matches!(args.output, TokenCreateOutput::Token) {
        info!("Successfully created new token");
    }

    match args.output {
        TokenCreateOutput::Table => output_details(GenericKeyValue::from_token(token)),
        TokenCreateOutput::Json => output_json(&token),
        TokenCreateOutput::Token => {
            println!("{}", token.token);
            Ok(())
        }
    }
}

async fn handle_token_list_command(args: ListArguments) -> Result<()> {
    let client = api_client_configuration(args.profile.as_deref()).await?;

    let tokens = token_list(
        &client,
        args.sort_by.map(Into::<&str>::into),
        args.sort_direction.map(Into::<&str>::into),
        args.page,
        args.limit,
    )
    .await?;

    match args.output {
        TokenListOutput::Table => {
            let rows: Vec<TokenRow> = tokens.into_iter().map(Into::into).collect();
            output_list(rows)
        }
        TokenListOutput::Json => output_json(&tokens),
    }
}

async fn handle_token_delete_command(args: DeleteArguments) -> Result<()> {
    let client = api_client_configuration(args.profile.as_deref()).await?;

    token_delete(&client, args.id).await?;

    info!("Successfully deleted token");
    Ok(())
}

#[derive(Table)]
struct TokenRow {
    #[table(title = "ID")]
    id: String,

    #[table(title = "Title")]
    title: String,

    #[table(title = "Created")]
    created_at: String,

    #[table(title = "Expires")]
    expires_at: String,
}

impl From<TokenSummary> for TokenRow {
    fn from(token: TokenSummary) -> Self {
        TokenRow {
            id: token.id.to_string(),
            title: token.title,
            created_at: token.created_at.format(&Rfc3339).unwrap_or_default(),
            expires_at: token.expires_at.map_or_else(String::new, |time| {
                time.format(&Rfc3339).unwrap_or_default()
            }),
        }
    }
}

impl GenericKeyValue {
    fn from_token(token: Token) -> Vec<Self> {
        vec![
            GenericKeyValue::new("ID:", token.id.to_string()),
            GenericKeyValue::new("Title:", token.title),
            GenericKeyValue::new("Token:", token.token),
            GenericKeyValue::new(
                "Created at:",
                token.created_at.format(&Rfc3339).unwrap_or_default(),
            ),
            GenericKeyValue::new(
                "Expires at:",
                token.expires_at.map_or_else(String::new, |time| {
                    time.format(&Rfc3339).unwrap_or_default()
                }),
            ),
        ]
    }
}
