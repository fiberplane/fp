use super::parse_logs::contains_logs;
use super::{parse_logs::parse_logs, Arguments};
use anyhow::{anyhow, Context, Error, Result};
use bytes::Bytes;
use fiberplane::protocols::core;
use fp_api_client::apis::configuration::Configuration;
use fp_api_client::apis::default_api::notebook_cells_append;
use fp_api_client::models::{Cell, CellAppendText};
use std::env::current_dir;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use tracing::debug;

#[derive(Debug, Clone, Copy, PartialEq)]
enum CellType {
    Log,
    Code,
    Unknown,
}

pub struct CellWriter {
    args: Arguments,
    config: Configuration,
    cell: Option<core::Cell>,
    buffer: Vec<u8>,
    /// At first, we don't know what type of cell we're writing to.
    /// We'll try to parse the data we get as a log and if it fails
    /// we'll assume we should write to a code cell.
    cell_type: CellType,
}

impl CellWriter {
    pub fn new(args: Arguments, config: Configuration) -> Self {
        Self {
            args,
            config,
            cell: None,
            buffer: Vec::new(),
            cell_type: CellType::Unknown,
        }
    }

    pub fn append(&mut self, data: Bytes) {
        self.buffer.extend_from_slice(&data);
    }

    /// Create a new cell and write the buffered text to it
    /// or append the buffered text to the cell if one was
    /// already created
    pub async fn flush(&mut self) -> Result<()> {
        if self.buffer.is_empty() {
            return Ok(());
        }

        self.detect_cell_type();

        let output = String::from_utf8_lossy(&self.buffer).to_string();

        match self.cell_type {
            CellType::Log => {
                let data = Some(parse_logs(&output));
                let cell = Cell::LogCell {
                    id: "".to_string(),
                    read_only: None,
                    source_ids: vec![],
                    title: self.prompt_line(),
                    formatting: None,
                    time_range: None,
                    data,
                };
                let cell = self.append_cell(cell).await?;
                self.cell = Some(cell);
            }
            // Create a new code cell
            CellType::Code | CellType::Unknown => {
                let content = format!("{}\n{}", self.prompt_line(), output);
                let cell = Cell::CodeCell {
                    id: String::new(),
                    content,
                    syntax: None,
                    read_only: None,
                };
                let cell = self.append_cell(cell).await?;
                self.cell = Some(cell);
            }
        }

        self.buffer.clear();

        Ok::<_, Error>(())
    }

    pub fn into_output_cell(self) -> Option<core::Cell> {
        self.cell
    }

    /// Try to parse the buffered data as a log and if it fails
    /// assume we should write to a code cell.
    fn detect_cell_type(&mut self) {
        if self.cell_type == CellType::Unknown {
            if let Ok(string) = std::str::from_utf8(&self.buffer) {
                if contains_logs(string) {
                    self.cell_type = CellType::Log;
                    debug!("Detected logs");
                } else {
                    self.cell_type = CellType::Code;
                    dbg!(&string);
                    debug!("Failed to detect logs, using code cell");
                }
            } else {
                debug!("Could not parse buffer as UTF-8");
            }
        }
    }

    async fn append_cell(&self, cell: Cell) -> Result<core::Cell> {
        let cell = notebook_cells_append(&self.config, &self.args.notebook_id, vec![cell])
            .await
            .with_context(|| "Error appending cell to notebook")?
            .pop()
            .ok_or_else(|| anyhow!("No cells returned"))?;
        let cell = serde_json::from_value(serde_json::to_value(cell)?)?;
        Ok(cell)
    }

    fn prompt_line(&self) -> String {
        let timestamp = OffsetDateTime::now_utc().format(&Rfc3339).unwrap();
        let cwd = current_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        format!(
            "{}\n{} ❯ {} {}",
            timestamp,
            cwd,
            self.args.command,
            self.args.args.join(" ")
        )
    }
}
