use anyhow::{anyhow, Context, Result};
use clap::Parser;
use fp_provider_runtime::spec::types::{Config, ProviderRequest, ProviderResponse};
use wasmer::{Singlepass, Store, Universal};

#[derive(Parser)]
pub struct Arguments {
    #[clap(subcommand)]
    subcmd: SubCommand,
}

pub async fn handle_command(args: Arguments) -> Result<()> {
    use SubCommand::*;
    match args.subcmd {
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
    #[clap(long, short, about = "path to the provider")]
    pub provider_path: String,

    #[clap(
        long,
        short,
        about = "JSON encoded request that will be sent to the provider"
    )]
    pub request: String,

    #[clap(
        long,
        short,
        about = "JSON encoded config that will be sent to the provider"
    )]
    pub config: String,
}

async fn handle_invoke_command(args: InvokeArguments) -> Result<()> {
    let request: ProviderRequest =
        serde_json::from_str(&args.request).context("unable to deserialize request")?;
    let config: Config =
        serde_json::from_str(&args.config).context("unable to deserialize config")?;

    let engine = Universal::new(Singlepass::default()).engine();
    let store = Store::new(&engine);

    let wasm_module = std::fs::read(args.provider_path)
        .map_err(|e| anyhow!("unable to read wasm module: {:?}", e))?;

    let runtime = fp_provider_runtime::Runtime::new(store, wasm_module)
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
