use super::{parse_logs::parse_logs, Arguments, CellType};
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

        match self.args.cell_type {
            CellType::Code => {
                // Either create a new cell or append to the existing one
                match &mut self.cell {
                    None => {
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
            }
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
        }
        Ok::<_, Error>(())
    }

    pub fn into_output_cell(self) -> Option<core::Cell> {
        self.cell
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
