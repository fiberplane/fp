use anyhow::{anyhow, Result};
use clap::Parser;
use std::error::Error;
use std::path::PathBuf;
use wasmer::{Singlepass, Store, Universal};

#[derive(Parser, Debug)]
pub struct Arguments {
    #[clap(about = "Payload to send to the template (format as JSON)")]
    payload: Option<String>,

    #[clap(long, env = "TEMPLATE_FILE", about = "Path to the template wasm")]
    template_file: PathBuf,

    #[clap(
        long,
        env = "DATA_SOURCES",
        parse(try_from_str = parse_key_val),
        multiple_occurrences(true),
        about = "Path to a data-sources.yaml file (these data-sources will be made available to the template)"
    )]
    data_sources: Option<Vec<(String, String)>>,

    #[clap(
        long,
        env = "WASM_DIR",
        about = "Path to the directory containing the providers (only required if data-sources is used)"
    )]
    providers_wasm_dir: Option<PathBuf>,
}

pub async fn handle_command(args: Arguments) -> Result<()> {
    if args.data_sources.is_some() && args.providers_wasm_dir.is_none() {
        return Err(anyhow!(
            "providers-wasm-dir is required if data-sources is used"
        ));
    }

    let engine = Universal::new(Singlepass::default()).engine();
    let store = Store::new(&engine);

    let runtime = {
        let template_wasm_module = std::fs::read(args.template_file)
            .map_err(|e| anyhow!("unable to read wasm module: {:?}", e))?;

        wasmer_template_runtime::Runtime::new(
            store,
            template_wasm_module,
            args.data_sources,
            args.providers_wasm_dir,
        )
        .map_err(|e| anyhow!("unable to create runtime: {:?}", e))?
    };

    println!("payload: {:?}", args.payload);

    let result = runtime.expand_template(args.payload).await;

    match result {
        // The inner result contains the result from the template
        Ok(Ok(notebook)) => {
            println!("{}", serde_json::to_string_pretty(&notebook)?);
            Ok(())
        }
        Ok(Err(err)) => Err(anyhow!("error happened within the template: {:?}", err)),
        // The outer result contains the result from the runtime
        Err(err) => Err(anyhow!("unable to invoke template: {:?}", err)),
    }
}

fn parse_key_val<T, U>(s: &str) -> Result<(T, U), Box<dyn Error + Send + Sync + 'static>>
where
    T: std::str::FromStr,
    T::Err: Error + Send + Sync + 'static,
    U: std::str::FromStr,
    U::Err: Error + Send + Sync + 'static,
{
    let pos = s
        .find('=')
        .ok_or_else(|| format!("invalid KEY=value: no `=` found in `{}`", s))?;
    Ok((s[..pos].parse()?, s[pos + 1..].parse()?))
}
