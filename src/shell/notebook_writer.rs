use anyhow::{anyhow, Result};
use fp_api_client::apis::configuration::Configuration;
use fp_api_client::apis::default_api::{
    get_profile, notebook_cell_append_text, notebook_cells_append,
};
use fp_api_client::models::{cell::HeadingType, Annotation, Cell, CellAppendText};
use once_cell::sync::OnceCell;
use time::{format_description::FormatItem, OffsetDateTime};

pub struct NotebookWriter {
    config: Configuration,
    notebook_id: String,
    code_cell_id: String,
    heading_cell_id: String,
}

fn get_ts_format() -> &'static (impl time::formatting::Formattable + ?Sized) {
    static TS_FORMAT: OnceCell<Vec<FormatItem<'static>>> = OnceCell::new();
    TS_FORMAT.get_or_init(|| {
        time::format_description::parse("[year]-[month]-[day] [hour]:[minute]:[second]").unwrap()
    })
}

impl NotebookWriter {
    pub async fn new(config: Configuration, notebook_id: String) -> Result<Self> {
        let user = get_profile(&config).await?;
        let header_cell = notebook_cells_append(
            &config,
            &notebook_id,
            vec![Cell::HeadingCell {
                id: String::new(),
                heading_type: HeadingType::H3,
                content: format!(
                    "@{}'s shell session\n🟢 Started at:\t{}",
                    user.name,
                    time::OffsetDateTime::now_utc()
                        .format(get_ts_format())
                        .unwrap()
                ),
                formatting: Some(vec![Annotation::MentionAnnotation {
                    offset: 0,
                    name: user.name,
                    user_id: user.id,
                }]),
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
        notebook_cell_append_text(
            &self.config,
            &self.notebook_id,
            &self.heading_cell_id,
            CellAppendText {
                content: format!(
                    "\n🔴 Ended at:\t{}",
                    OffsetDateTime::now_utc().format(get_ts_format()).unwrap()
                ),
                formatting: None,
            },
        )
        .await?;
        Ok(())
    }
}
