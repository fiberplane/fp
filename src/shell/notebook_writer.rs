use anyhow::{anyhow, Result};
use fiberplane::api_client::apis::configuration::Configuration;
use fiberplane::api_client::apis::default_api::{
    notebook_cell_append_text, notebook_cells_append, profile_get,
};
use fiberplane::api_client::models::{cell::HeadingType, Annotation, Cell, CellAppendText};
use fiberplane::string_utils::char_count;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

pub struct NotebookWriter {
    config: Configuration,
    notebook_id: String,
    code_cell_id: String,
    heading_cell_id: String,
}

impl NotebookWriter {
    pub async fn new(config: Configuration, notebook_id: String) -> Result<Self> {
        let user = profile_get(&config).await?;
        let timestamp = time::OffsetDateTime::now_utc().format(&Rfc3339).unwrap();
        let content = format!(
            "@{}'s shell session\n🟢 Started at:\t{}",
            user.name, timestamp
        );
        let timestamp_offset = char_count(&content) - char_count(&timestamp);
        let header_cell = notebook_cells_append(
            &config,
            &notebook_id,
            vec![Cell::HeadingCell {
                id: String::new(),
                heading_type: HeadingType::H3,
                content,
                formatting: Some(vec![
                    Annotation::MentionAnnotation {
                        offset: 0,
                        name: user.name,
                        user_id: user.id,
                    },
                    Annotation::TimestampAnnotation {
                        offset: timestamp_offset as i32,
                        timestamp,
                    },
                ]),
                read_only: Some(true),
            }],
        )
        .await?
        .pop()
        .ok_or_else(|| anyhow!("No cells returned"))?;

        let code_cell = notebook_cells_append(
            &config,
            &notebook_id,
            vec![Cell::CodeCell {
                id: String::new(),
                content: String::new(),
                read_only: Some(true),
                syntax: None,
            }],
        )
        .await?
        .pop()
        .ok_or_else(|| anyhow!("No cells returned"))?;

        let code_cell_id = match code_cell {
            Cell::CodeCell { id, .. } => id,
            _ => unreachable!(),
        };

        let heading_cell_id = match header_cell {
            Cell::HeadingCell { id, .. } => id,
            _ => unreachable!(),
        };

        Ok(Self {
            config,
            notebook_id,
            code_cell_id,
            heading_cell_id,
        })
    }

    pub async fn write(&self, buffer: Vec<u8>) -> Result<()> {
        let content = String::from_utf8(buffer)?;

        notebook_cell_append_text(
            &self.config,
            &self.notebook_id,
            &self.code_cell_id,
            CellAppendText {
                content,
                formatting: None,
            },
        )
        .await?;

        Ok(())
    }

    pub async fn close(&self) -> Result<()> {
        let timestamp = OffsetDateTime::now_utc().format(&Rfc3339).unwrap();
        let content = format!("\n🔴 Ended at: \t{}", timestamp);
        let timestamp_offset = char_count(&content) - char_count(&timestamp);
        notebook_cell_append_text(
            &self.config,
            &self.notebook_id,
            &self.heading_cell_id,
            CellAppendText {
                content,
                formatting: Some(vec![Annotation::TimestampAnnotation {
                    offset: timestamp_offset as i32,
                    timestamp,
                }]),
            },
        )
        .await?;
        Ok(())
    }
}
