use anyhow::{anyhow, bail, Context, Result};
use clap::Parser;
use fiberplane::provider_runtime::spec::types::{
    Blob, LegacyProviderRequest, LegacyProviderResponse, ProviderConfig, ProviderRequest,
};

#[derive(Parser)]
pub struct Arguments {
    #[clap(subcommand)]
    sub_command: SubCommand,
}

pub async fn handle_command(args: Arguments) -> Result<()> {
    use SubCommand::*;
    match args.sub_command {
        Invoke(args) => handle_invoke_command(args).await,
        Invoke2(args) => handle_invoke2_command(args).await,
    }
}

#[derive(Parser)]
pub enum SubCommand {
    /// Invoke a provider with the legacy provider protocol
    Invoke(InvokeArguments),

    /// Invoke a provider with the new provider protocol
    Invoke2(Invoke2Arguments),
}

#[derive(Parser, Debug)]
pub struct InvokeArguments {
    /// Path to the provider WASM file
    #[clap(long, short)]
    pub provider_path: String,

    /// JSON encoded request that will be sent to the provider
    #[clap(long, short)]
    pub request: String,

    /// JSON encoded config that will be sent to the provider
    #[clap(long, short)]
    pub config: String,
}

async fn handle_invoke_command(args: InvokeArguments) -> Result<()> {
    let request: LegacyProviderRequest =
        serde_json::from_str(&args.request).context("unable to deserialize request")?;
    let config = parse_config(&args.config).context("unable to deserialize config")?;

    let wasm_module = std::fs::read(args.provider_path)
        .map_err(|e| anyhow!("unable to read wasm module: {:?}", e))?;

    let runtime = fiberplane::provider_runtime::spec::Runtime::new(wasm_module)
        .map_err(|e| anyhow!("unable to create runtime: {:?}", e))?;

    let result = runtime.invoke(request, config).await;

    match result {
        Ok(LegacyProviderResponse::Error { error: err }) => {
            Err(anyhow!("Provider failed: {:?}", err))
        }
        Ok(val) => match serde_json::to_string_pretty(&val) {
            Ok(val) => {
                println!("{}", val);
                Ok(())
            }
            Err(e) => Err(anyhow!("unable to serialize result: {:?}", e)),
        },
        Err(e) => Err(anyhow!("unable to invoke provider: {:?}", e)),
    }
}

#[derive(Parser, Debug)]
pub struct Invoke2Arguments {
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

async fn handle_invoke2_command(args: Invoke2Arguments) -> Result<()> {
    let config = parse_config(&args.config).context("unable to deserialize config")?;
    let request = ProviderRequest {
        query_type: args.query_type,
        query_data: Blob {
            data: args.query_data.into(),
            mime_type: args.query_mime_type,
        },
        config,
        previous_response: None,
    };

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
                println!("{}", base64::encode(blob.data.as_ref()));
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
