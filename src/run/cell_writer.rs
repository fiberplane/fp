use super::parse_logs::{contains_logs, parse_logs};
use anyhow::{anyhow, Context, Result};
use bytes::Bytes;
use fiberplane::api_client::clients::ApiClient;
use fiberplane::api_client::notebook_cells_append;
use fiberplane::base64uuid::Base64Uuid;
use fiberplane::models::notebooks;
use fiberplane::models::notebooks::{Cell, CodeCell, LogCell, TextCell};
use std::env::current_dir;
use std::vec;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use tracing::debug;

#[derive(Debug, Clone, Copy, PartialEq)]
enum CellType {
    Log,
    Code,
    Unknown,
}

pub struct CellWriter {
    notebook_id: Base64Uuid,
    command: Vec<String>,
    client: ApiClient,
    cell: Option<notebooks::Cell>,
    buffer: Vec<u8>,
    /// At first, we don't know what type of cell we're writing to.
    /// We'll try to parse the data we get as a log and if it fails
    /// we'll assume we should write to a code cell.
    cell_type: CellType,
}

impl CellWriter {
    pub fn new(config: ApiClient, notebook_id: Base64Uuid, command: Vec<String>) -> Self {
        Self {
            notebook_id,
            command,
            client: config,
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
                // Prepend a text cell with the "title":
                let cell = Cell::Text(TextCell {
                    id: String::new(),
                    content: self.prompt_line(),
                    formatting: vec![],
                    read_only: None,
                });
                self.append_cell(cell).await?;

                // Followed by the log cell itself:
                let data = parse_logs(&output);
                let data_link = format!(
                    "data:application/vnd.fiberplane.events+json,{}",
                    serde_json::to_string(&data).expect("Could not serialize log records")
                );

                let cell = Cell::Log(LogCell {
                    id: String::new(),
                    data_links: vec![data_link],
                    read_only: Some(true),
                    display_fields: None,
                    expanded_indices: None,
                    hide_similar_values: None,
                    highlighted_indices: None,
                    selected_indices: None,
                    visibility_filter: None,
                });
                let cell = self.append_cell(cell).await?;
                self.cell = Some(cell);
            }
            // Create a new code cell
            CellType::Code | CellType::Unknown => {
                let content = format!("{}\n{}", self.prompt_line(), output);
                let cell = Cell::Code(CodeCell {
                    id: String::new(),
                    content,
                    syntax: None,
                    read_only: None,
                });
                let cell = self.append_cell(cell).await?;
                self.cell = Some(cell);
            }
        }

        self.buffer.clear();

        Ok(())
    }

    pub fn into_output_cell(self) -> Option<notebooks::Cell> {
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
                    debug!("Failed to detect logs, using code cell");
                }
            } else {
                debug!("Could not parse buffer as UTF-8");
            }
        }
    }

    async fn append_cell(&self, cell: Cell) -> Result<Cell> {
        let cell = notebook_cells_append(&self.client, self.notebook_id, None, None, vec![cell])
            .await
            .with_context(|| "Error appending cell to notebook")?
            .pop()
            .ok_or_else(|| anyhow!("No cells returned"))?;

        Ok(cell)
    }

    fn prompt_line(&self) -> String {
        let timestamp = OffsetDateTime::now_utc().format(&Rfc3339).unwrap();
        let cwd = current_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        format!("{}\n{} \u{276f} {}", timestamp, cwd, self.command.join(" "),)
    }
}
