use anyhow::{anyhow, Result};
use fiberplane::api_client::clients::ApiClient;
use fiberplane::api_client::{notebook_cell_append_text, notebook_cells_append, profile_get};
use fiberplane::base64uuid::Base64Uuid;
use fiberplane::models::formatting::{Annotation, AnnotationWithOffset, Formatting, Mention};
use fiberplane::models::notebooks::operations::CellAppendText;
use fiberplane::models::notebooks::{Cell, CodeCell, HeadingCell, HeadingType};
use fiberplane::models::utils::char_count;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

pub struct NotebookWriter {
    config: ApiClient,
    notebook_id: Base64Uuid,
    code_cell_id: String,
    heading_cell_id: String,
}

impl NotebookWriter {
    pub async fn new(config: ApiClient, notebook_id: Base64Uuid) -> Result<Self> {
        let user = profile_get(&config).await?;

        let raw_timestamp = OffsetDateTime::now_utc();
        let timestamp = raw_timestamp.format(&Rfc3339).unwrap();

        let content = format!(
            "@{}'s shell session\n🟢 Started at:\t{}",
            user.name, timestamp
        );
        let timestamp_offset = char_count(&content) - char_count(&timestamp);
        let header_cell = notebook_cells_append(
            &config,
            notebook_id,
            None,
            None,
            vec![Cell::Heading(HeadingCell::builder()
                                   .id(String::new())
                                   .heading_type(HeadingType::H3)
                                   .content(content)
                                   .formatting(vec![
                    AnnotationWithOffset::new(0, Annotation::Mention(Mention::builder()
                                                                         .name(user.name)
                                                                         .user_id(user.id.to_string())
                        .build())
                    ),
                    AnnotationWithOffset::new(timestamp_offset, Annotation::Timestamp {
                            timestamp: raw_timestamp,
                        }
                    ),
                ])
                                   .read_only(true)
                .build())],
        )
        .await?
        .pop()
        .ok_or_else(|| anyhow!("No cells returned"))?;

        let code_cell = notebook_cells_append(
            &config,
            notebook_id,
            None,
            None,
            vec![Cell::Code(CodeCell::builder()
                                .id(String::new())
                                .content(String::new())
                                .read_only(true)
                .build())],
        )
        .await?
        .pop()
        .ok_or_else(|| anyhow!("No cells returned"))?;

        let code_cell_id = match code_cell {
            Cell::Code(CodeCell { id, .. }) => id,
            _ => unreachable!(),
        };

        let heading_cell_id = match header_cell {
            Cell::Heading(HeadingCell { id, .. }) => id,
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
            self.notebook_id,
            &self.code_cell_id,
            CellAppendText::builder()
                .content(content)
                .formatting(Formatting::new())
                .build(),
        )
        .await?;

        Ok(())
    }

    pub async fn close(&self) -> Result<()> {
        let now = OffsetDateTime::now_utc();
        let timestamp = now.format(&Rfc3339).unwrap();
        let content = format!("\n🔴 Ended at: \t{timestamp}");

        notebook_cell_append_text(
            &self.config,
            self.notebook_id,
            &self.heading_cell_id,
            CellAppendText::builder()
                .content(content)
                .formatting(vec![AnnotationWithOffset::new(
                    0,
                    Annotation::Timestamp { timestamp: now },
                )])
                .build(),
        )
        .await?;

        Ok(())
    }
}
