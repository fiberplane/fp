use anyhow::{anyhow, Result};
use clap::Parser;
use fp_provider_runtime::spec::types::{DataSource, PrometheusDataSource, QueryInstantOptions};
use std::time::{SystemTime, UNIX_EPOCH};
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
    #[clap(
        name = "invoke",
        about = "Invoke a provider (only Prometheus is supported)"
    )]
    Invoke(InvokeArguments),
}

#[derive(Parser, Debug)]
pub struct InvokeArguments {
    #[clap(name = "provider_path", long, short, about = "path to the provider")]
    pub provider_path: String,

    #[clap(name = "query", about = "query that will be sent to the provider")]
    pub query: String,

    #[clap(name = "url", long, short, about = "URL to the Prometheus instance")]
    pub prometheus_url: String,
}

async fn handle_invoke_command(args: InvokeArguments) -> Result<()> {
    let engine = Universal::new(Singlepass::default()).engine();
    let store = Store::new(&engine);

    let wasm_module = std::fs::read(args.provider_path)
        .map_err(|e| anyhow!("unable to read wasm module: {:?}", e))?;

    let runtime = fp_provider_runtime::Runtime::new(store, wasm_module)
        .map_err(|e| anyhow!("unable to create runtime: {:?}", e))?;

    // TODO: it should be possible to specify the instant through an argument,
    // the following should be used if no argument was used.
    let time = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_secs() as f64 + duration.subsec_nanos() as f64 * 1e-9,
        Err(_) => {
            eprintln!("System time is set before epoch! Returning epoch as fallback.");
            0_f64
        }
    };

    let query = args.query;
    let data_source = DataSource::Prometheus(PrometheusDataSource {
        url: args.prometheus_url,
    });
    let options = QueryInstantOptions { data_source, time };
    let result = runtime.fetch_instant(query, options).await;

    match result {
        Ok(val) => match val {
            Ok(val) => match serde_json::to_string_pretty(&val) {
                Ok(val) => {
                    println!("{}", val);
                    Ok(())
                }
                Err(e) => Err(anyhow!("unable to serialize result: {:?}", e)),
            },
            Err(e) => Err(anyhow!("Provider failed: {:?}", e)),
        },
        Err(e) => Err(anyhow!("Unable to invoke provider: {:?}", e)),
    }
}
