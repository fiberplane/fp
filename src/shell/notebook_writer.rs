use anyhow::{anyhow, Result};
use fp_api_client::apis::default_api::{
    get_profile, notebook_cell_append_text, notebook_cells_append, NotebookCellAppendTextError,
};
use fp_api_client::apis::{configuration::Configuration, Error};
use fp_api_client::models::{cell::HeadingType, Annotation, Cell, CellAppendText};
use once_cell::sync::OnceCell;
use pin_project::pin_project;
use std::{future::Future, pin::Pin, sync::Arc, task::Poll};
use time::{format_description::FormatItem, OffsetDateTime};

struct Inner {
    config: Configuration,
    notebook_id: String,
    code_cell_id: String,
    heading_cell_id: String,
}

#[pin_project]
pub struct NotebookWriter {
    #[pin]
    inner: Arc<Inner>,
    #[pin]
    buffer: Vec<u8>,
    future: Option<Pin<Box<dyn Future<Output = Result<Cell, Error<NotebookCellAppendTextError>>>>>>,
}

static TS_FORMAT: OnceCell<Vec<FormatItem<'static>>> = OnceCell::new();

fn get_ts_format() -> &'static (impl time::formatting::Formattable + ?Sized) {
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
            inner: Arc::new(Inner {
                config,
                notebook_id,
                code_cell_id,
                heading_cell_id,
            }),
            buffer: Vec::with_capacity(4096),
            future: None,
        })
    }

    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    pub async fn close(&self) -> Result<()> {
        notebook_cell_append_text(
            &self.inner.config,
            &self.inner.notebook_id,
            &self.inner.heading_cell_id,
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

impl tokio::io::AsyncWrite for NotebookWriter {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<Result<usize, std::io::Error>> {
        let this = self.project();
        tokio::io::AsyncWrite::poll_write(this.buffer, cx, buf)
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        let this = self.project();

        let inner = this.inner.clone();
        let buf = this.buffer.get_mut();

        let fut = this.future.get_or_insert_with(|| {
            let content = String::from_utf8_lossy(buf).to_string();
            buf.clear();

            Box::pin(async move {
                let inner = inner;

                notebook_cell_append_text(
                    &inner.config,
                    &inner.notebook_id,
                    &inner.code_cell_id,
                    CellAppendText {
                        content,
                        formatting: None,
                    },
                )
                .await
            })
        });

        match fut.as_mut().poll(cx) {
            Poll::Ready(_res) => {
                *this.future = None; //future is done polling so we eat it
                Poll::Ready(Ok(()))
            }
            Poll::Pending => Poll::Pending,
        }
    }

    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), std::io::Error>> {
        let this = self.project();
        tokio::io::AsyncWrite::poll_shutdown(this.buffer, cx)
    }
}
