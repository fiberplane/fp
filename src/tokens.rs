use crate::config::api_client_configuration;
use crate::output::{output_details, output_json, output_list, GenericKeyValue};
use anyhow::Result;
use clap::{ArgEnum, Parser};
use cli_table::Table;
use fiberplane::sorting::{SortDirection, TokenListSortFields};
use fp_api_client::apis::default_api::{token_create, token_delete, token_list};
use fp_api_client::models::{NewToken, Token, TokenSummary};
use std::path::PathBuf;
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
    Create(CreateArguments),

    /// Lists all tokens
    List(ListArguments),

    /// Deletes a token
    Delete(DeleteArguments),
}

#[derive(ArgEnum, Clone)]
enum TokenCreateOutput {
    /// Output the details as a table
    Table,

    /// Output the details as JSON
    Json,

    /// Output only the token
    Token,
}

#[derive(ArgEnum, Clone)]
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
    #[clap(long, short, default_value = "table", arg_enum)]
    output: TokenCreateOutput,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

#[derive(Parser)]
pub struct ListArguments {
    /// Output of the token
    #[clap(long, short, default_value = "table", arg_enum)]
    output: TokenListOutput,

    /// Sort the result according to the following field
    #[clap(long, arg_enum)]
    sort_by: Option<TokenListSortFields>,

    /// Sort the result in the following direction
    #[clap(long, arg_enum)]
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
    config: Option<PathBuf>,
}

#[derive(Parser)]
pub struct DeleteArguments {
    /// ID of the token that should be deleted
    id: String,

    #[clap(from_global)]
    base_url: Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}

async fn handle_token_create_command(args: CreateArguments) -> Result<()> {
    let config = api_client_configuration(args.config, &args.base_url).await?;

    let token = token_create(&config, NewToken::new(args.name)).await?;

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
    let config = api_client_configuration(args.config, &args.base_url).await?;

    let tokens = token_list(
        &config,
        args.sort_by.map(Into::into),
        args.sort_direction.map(Into::into),
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
    let config = api_client_configuration(args.config, &args.base_url).await?;

    token_delete(&config, &args.id).await?;

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
            id: token.id,
            title: token.title,
            created_at: token.created_at,
            expires_at: token.expires_at.unwrap_or_else(|| "Never".to_string()),
        }
    }
}

impl GenericKeyValue {
    fn from_token(token: Token) -> Vec<Self> {
        vec![GenericKeyValue::new("Token:", token.token)]
    }
}
