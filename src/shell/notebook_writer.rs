use anyhow::{anyhow, Result};
use fiberplane::api_client::ApiClient;
use fiberplane::base64uuid::Base64Uuid;
use fiberplane::models::formatting::{Annotation, AnnotationWithOffset, Formatting, Mention};
use fiberplane::models::notebooks::operations::CellAppendText;
use fiberplane::models::notebooks::{Cell, CodeCell, HeadingCell, HeadingType};
use fiberplane::models::timestamps::Timestamp;
use fiberplane::models::utils::char_count;

pub struct NotebookWriter {
    client: ApiClient,
    notebook_id: Base64Uuid,
    code_cell_id: String,
    heading_cell_id: String,
}

impl NotebookWriter {
    pub async fn new(client: ApiClient, notebook_id: Base64Uuid) -> Result<Self> {
        let user = client.profile_get().await?;

        let now = Timestamp::now_utc();
        let timestamp = now.to_string();

        let content = format!(
            "@{}'s shell session\n🟢 Started at:\t{}",
            user.name, timestamp
        );
        let timestamp_offset = char_count(&content) - char_count(&timestamp);
        let header_cell = client
            .notebook_cells_append(
                notebook_id,
                None,
                None,
                vec![Cell::Heading(
                    HeadingCell::builder()
                        .id(String::new())
                        .heading_type(HeadingType::H3)
                        .content(content)
                        .formatting(vec![
                            AnnotationWithOffset::new(
                                0,
                                Annotation::Mention(
                                    Mention::builder().name(user.name).user_id(user.id).build(),
                                ),
                            ),
                            AnnotationWithOffset::new(
                                timestamp_offset,
                                Annotation::Timestamp { timestamp: now },
                            ),
                        ])
                        .read_only(true)
                        .build(),
                )],
            )
            .await?
            .pop()
            .ok_or_else(|| anyhow!("No cells returned"))?;

        let code_cell = client
            .notebook_cells_append(
                notebook_id,
                None,
                None,
                vec![Cell::Code(
                    CodeCell::builder()
                        .id(String::new())
                        .content(String::new())
                        .read_only(true)
                        .build(),
                )],
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
            client,
            notebook_id,
            code_cell_id,
            heading_cell_id,
        })
    }

    pub async fn write(&self, buffer: Vec<u8>) -> Result<()> {
        let content = String::from_utf8(buffer)?;

        self.client
            .notebook_cell_append_text(
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
        let now = Timestamp::now_utc();
        let content = format!("\n🔴 Ended at: \t{now}");

        self.client
            .notebook_cell_append_text(
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
