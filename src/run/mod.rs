use anyhow::Result;
use clap::Parser;

#[derive(Parser, Clone)]
pub struct Arguments {
    /// Path to the script to run.
    path: String,
}

pub async fn handle_command(args: Arguments) -> Result<()> {
    fiberscript::run_script(args.path).await
}
