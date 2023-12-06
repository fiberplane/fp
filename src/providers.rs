use anyhow::{anyhow, bail, Context, Result};
use base64::prelude::*;
use clap::Parser;
use fiberplane::provider_runtime::spec::types::{Blob, ProviderConfig, ProviderRequest};

#[derive(Parser)]
pub struct Arguments {
    #[clap(subcommand)]
    sub_command: SubCommand,
}

pub async fn handle_command(args: Arguments) -> Result<()> {
    use SubCommand::*;
    match args.sub_command {
        Invoke(args) => handle_invoke2_command(args).await,
    }
}

#[derive(Parser)]
pub enum SubCommand {
    /// Invoke a provider with the new provider protocol
    #[clap(alias = "invoke2")]
    Invoke(InvokeArguments),
}

#[derive(Parser, Debug)]
pub struct InvokeArguments {
    /// Path to the provider WASM file
    #[clap(long, short)]
    pub provider_path: String,

    /// JSON encoded request that will be sent to the provider
    #[clap(long, short)]
    pub request: String,

    /// Type of query for the provider (available options are set by the provider)
    #[clap(long, short = 't')]
    pub query_type: String,

    /// Data to be sent to the provider
    #[clap(long, short = 'q')]
    pub query_data: Vec<u8>,

    /// Mime type of the query data
    #[clap(long, short = 'm', default_value = "application/x-www-form-urlencoded")]
    pub query_mime_type: String,

    /// JSON encoded config that will be sent to the provider
    #[clap(long, short)]
    pub config: String,
}

async fn handle_invoke2_command(args: InvokeArguments) -> Result<()> {
    let config = parse_config(&args.config).context("unable to deserialize config")?;
    let request = ProviderRequest::builder()
        .query_type(args.query_type)
        .query_data(
            Blob::builder()
                .data(args.query_data)
                .mime_type(args.query_mime_type)
                .build(),
        )
        .config(config)
        .build();

    let wasm_module = std::fs::read(args.provider_path)
        .map_err(|e| anyhow!("unable to read wasm module: {:?}", e))?;

    let runtime = fiberplane::provider_runtime::spec::Runtime::new(wasm_module)
        .map_err(|e| anyhow!("unable to create runtime: {:?}", e))?;

    let result = runtime.invoke2(request).await;

    match result {
        Ok(Ok(blob)) => {
            if blob.mime_type.ends_with("json") {
                let json: serde_json::Value = serde_json::from_slice(blob.data.as_ref())?;
                println!("{}", serde_json::to_string_pretty(&json)?);
            } else if blob.mime_type.ends_with("msgpack") {
                let value: serde_json::Value = rmp_serde::from_slice(blob.data.as_ref())
                    .context("Unable to transcode MessagePack to JSON")?;
                println!("{}", serde_json::to_string_pretty(&value)?);
            } else {
                println!("{}", BASE64_STANDARD.encode(blob.data.as_ref()));
            }
            Ok(())
        }
        Ok(Err(err)) => bail!("Provider failed: {:?}", err),
        Err(e) => bail!("unable to invoke provider: {:?}", e),
    }
}

fn parse_config(json: &str) -> Result<ProviderConfig> {
    serde_json::from_str(json).map_err(serde_json::Error::into)
}
