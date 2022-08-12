use anyhow::{anyhow, Context, Result};
use clap::Parser;
use fp_provider_runtime::spec::types::{Blob, ProviderRequest};
use serde_bytes::ByteBuf;

#[derive(Parser)]
pub struct Arguments {
    #[clap(subcommand)]
    sub_command: SubCommand,
}

pub async fn handle_command(args: Arguments) -> Result<()> {
    use SubCommand::*;
    match args.sub_command {
        Invoke(args) => handle_invoke_command(args).await,
    }
}

#[derive(Parser)]
pub enum SubCommand {
    #[clap(name = "invoke", about = "Invoke a provider")]
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
    #[clap(long, short)]
    pub query_type: String,

    /// Data to be sent to the provider
    #[clap(long, short = 'd')]
    pub query_data: Vec<u8>,

    /// Mime type of the query data
    #[clap(long, short, default_value = "application/x-www-form-urlencoded")]
    pub query_mime_type: String,

    /// JSON encoded config that will be sent to the provider
    #[clap(long, short)]
    pub config: String,
}

async fn handle_invoke_command(args: InvokeArguments) -> Result<()> {
    let config = json_to_messagepack(&args.config).context("unable to deserialize config")?;
    let request = ProviderRequest {
        query_type: args.query_type,
        query_data: Blob {
            data: ByteBuf::from(args.query_data),
            mime_type: args.query_mime_type,
        },
        config,
        previous_response: None,
    };

    let wasm_module = std::fs::read(args.provider_path)
        .map_err(|e| anyhow!("unable to read wasm module: {:?}", e))?;

    let runtime = fp_provider_runtime::spec::Runtime::new(wasm_module)
        .map_err(|e| anyhow!("unable to create runtime: {:?}", e))?;

    let result = runtime.invoke2(request).await;

    match result {
        Ok(Ok(blob)) => {
            if blob.mime_type.ends_with("json") {
                let json = serde_json::from_slice(blob.data.as_ref())?;
                println!("{}", serde_json::to_string_pretty(&json)?);
            } else if blob.mime_type.ends_with("msgpack") {
                let value: serde_json::Value = rmp_serde::from_slice(blob.data.as_ref())
                    .with_context(|| "Unable to transcode MessagePack to JSON")?;
                println!("{}", serde_json::to_string_pretty(&value)?);
            } else {
                println!("{}", base64::encode(blob.data.as_ref()));
            }
            Ok(())
        }
        Ok(Err(err)) => Err(anyhow!("Provider failed: {:?}", err)),
        Err(e) => Err(anyhow!("unable to invoke provider: {:?}", e)),
    }
}

/// Transcode JSON to messagepack using serde-transcode
fn json_to_messagepack(json: &str) -> Result<rmpv::Value> {
    let value: serde_json::Value = serde_json::from_str(json)?;
    rmpv::ext::to_value(value).map_err(|e| e.into())
}
