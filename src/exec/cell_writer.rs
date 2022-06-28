use super::Arguments;
use anyhow::{anyhow, Context, Error, Result};
use bytes::Bytes;
use fiberplane::protocols::core;
use fp_api_client::apis::configuration::Configuration;
use fp_api_client::apis::default_api::{notebook_cell_append_text, notebook_cells_append};
use fp_api_client::models::{Cell, CellAppendText};
use std::env::current_dir;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

pub struct CellWriter {
    args: Arguments,
    config: Configuration,
    cell: Option<core::Cell>,
    buffer: Vec<Bytes>,
}

impl CellWriter {
    pub fn new(args: Arguments, config: Configuration) -> Self {
        Self {
            args,
            config,
            cell: None,
            buffer: Vec::new(),
        }
    }

    pub fn append(&mut self, data: Bytes) {
        self.buffer.push(data);
    }

    /// Create a new cell and write the buffered text to it
    /// or append the buffered text to the cell if one was
    /// already created
    pub async fn write_to_cell(&mut self) -> Result<()> {
        if self.buffer.is_empty() {
            return Ok(());
        }

        let mut output = String::new();
        let buffer = self.buffer.split_off(0);
        for chunk in buffer {
            output.push_str(&String::from_utf8_lossy(&chunk));
        }

        // Either create a new cell or append to the existing one
        match &mut self.cell {
            None => {
                let timestamp = OffsetDateTime::now_utc().format(&Rfc3339)?;
                let cwd = current_dir()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default();
                let content = format!(
                    "{}\n{} ❯ {} {}\n{}",
                    timestamp,
                    cwd,
                    self.args.command,
                    self.args.args.join(" "),
                    output
                );
                let cell = Cell::CodeCell {
                    id: String::new(),
                    content,
                    syntax: None,
                    read_only: None,
                };

                let cell = notebook_cells_append(&self.config, &self.args.notebook_id, vec![cell])
                    .await
                    .with_context(|| "Error appending cell to notebook")?
                    .pop()
                    .ok_or_else(|| anyhow!("No cells returned"))?;
                self.cell = Some(serde_json::from_value(serde_json::to_value(cell)?)?);
            }
            Some(cell) => {
                notebook_cell_append_text(
                    &self.config,
                    &self.args.notebook_id,
                    cell.id(),
                    CellAppendText {
                        content: output,
                        formatting: None,
                    },
                )
                .await
                .with_context(|| format!("Error appending text to cell {}", cell.id()))?;
            }
        }
        Ok::<_, Error>(())
    }

    pub fn into_output_cell(self) -> Option<core::Cell> {
        self.cell
    }
}
