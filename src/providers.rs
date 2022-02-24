use anyhow::{anyhow, Context, Result};
use clap::Parser;
use fp_provider_runtime::spec::types::{ProviderRequest, ProviderResponse};

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

    /// JSON encoded config that will be sent to the provider
    #[clap(long, short)]
    pub config: String,
}

async fn handle_invoke_command(args: InvokeArguments) -> Result<()> {
    let request: ProviderRequest =
        serde_json::from_str(&args.request).context("unable to deserialize request")?;
    let config = json_to_messagepack(&args.config).context("unable to deserialize config")?;

    let wasm_module = std::fs::read(args.provider_path)
        .map_err(|e| anyhow!("unable to read wasm module: {:?}", e))?;

    let runtime = fp_provider_runtime::spec::Runtime::new(wasm_module)
        .map_err(|e| anyhow!("unable to create runtime: {:?}", e))?;

    let result = runtime.invoke(request, config).await;

    match result {
        Ok(ProviderResponse::Error { error: err }) => Err(anyhow!("Provider failed: {:?}", err)),
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

/// Transcode JSON to messagepack using serde-transcode
fn json_to_messagepack(json: &str) -> Result<rmpv::Value> {
    let value: serde_json::Value = serde_json::from_str(json)?;
    rmpv::ext::to_value(value).map_err(|e| e.into())
}
