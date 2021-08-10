use clap::Clap;
use fp_provider_runtime::spec::types::QueryInstantOptions;
use std::time::{SystemTime, UNIX_EPOCH};
use wasmer::{Singlepass, Store, Universal};

#[derive(Clap)]
pub struct Arguments {
    #[clap(subcommand)]
    subcmd: SubCommand,
}

pub async fn handle_command(args: Arguments) {
    use SubCommand::*;
    match args.subcmd {
        Invoke(args) => handle_invoke_command(args).await,
    }
}

#[derive(Clap)]
pub enum SubCommand {
    #[clap(name = "invoke", about = "Invoke a provider")]
    Invoke(InvokeArguments),
}

#[derive(Clap, Debug)]
pub struct InvokeArguments {
    #[clap(name = "provider_path", long, short, about = "path to the provider")]
    pub provider_path: String,

    #[clap(name = "query", about = "query that will be send to the provider")]
    pub query: String,
}

async fn handle_invoke_command(args: InvokeArguments) {
    let engine = Universal::new(Singlepass::default()).engine();
    let store = Store::new(&engine);

    let wasm_module = std::fs::read(args.provider_path).expect("unable to read wasm module");

    let runtime =
        fp_provider_runtime::Runtime::new(store, wasm_module).expect("unable to create runtime");

    // TODO: it should be possible to specify the instant through an argument,
    // the following should be used if no argument was used.
    let time = {
        match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(duration) => duration.as_secs() as f64 + duration.subsec_nanos() as f64 * 1e-9,
            Err(_) => {
                eprintln!("System time is set before epoch! Returning epoch as fallback.");
                0_f64
            }
        }
    };

    let query = args.query;
    let options = QueryInstantOptions { time };
    let result = runtime.fetch_instant(query, options).await;

    match result {
        Ok(val) => match val {
            Ok(val) => println!("Received {} series", val.len()),
            Err(e) => eprintln!("Provider failed: {:?}", e),
        },
        Err(e) => eprintln!("Unable to invoke provider: {:?}", e),
    }
}

//transcode:
// println!("plugins -> invoke | I was called: {:?}", args);

// let payload = &args.payload.unwrap();
// let mut deserializer = serde_json::Deserializer::from_str(payload);

// // A compacted JSON serializer. You can use any Serde Serializer here.
// let mut serializer = rmp_serde::Serializer::new(io::stdout());

// serde_transcode::transcode(&mut deserializer, &mut serializer).unwrap();
