mod parser_iter;
mod pty_terminal;
pub mod shell_type;
mod terminal_render;
mod text_render;

use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
pub struct Arguments {
    // ID of the notebook
    #[clap(name = "id", env = "__FP_NOTEBOOK_ID")]
    id: String,

    #[clap(default_value_t = false, parse(from_flag), env = "__FP_SHELL_SESSION")]
    nested: bool,

    #[clap(from_global)]
    base_url: url::Url,

    #[clap(from_global)]
    config: Option<PathBuf>,
}
