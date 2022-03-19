use anyhow::{anyhow, Result};
use fiberplane::{
    operations::*,
    protocols::{
        core::*,
        formatting::{Annotation, AnnotationWithOffset, Mention},
        operations::*,
        realtime::{
            self, ApplyOperationBatchMessage, ApplyOperationMessage, ClientRealtimeMessage,
            ServerRealtimeMessage,
        },
    },
};
use futures_util::{
    stream::{SplitSink, SplitStream},
    SinkExt, StreamExt,
};
use rand::prelude::*;
use std::{collections::BTreeMap, convert::TryInto, ops::Range, time::Duration};
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};
use time::OffsetDateTime;
use tokio::{net::TcpStream, sync::oneshot};
use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::config::api_client_configuration;

#[derive(Debug, Clone)]
struct MemoryNotebook(Notebook);

impl MemoryNotebook {
    /// Applies the given operation to this notebook.
    pub fn apply_operation(&self, operation: &Operation) -> Result<Self, Error> {
        Ok(self.clone().apply_changes(apply_operation(
            &self.state_for_operation(operation),
            operation,
        )?))
    }

    fn apply_changes(self, changes: Vec<Change>) -> Self {
        let mut notebook = self;
        for change in changes.into_iter() {
            notebook = notebook.apply_change(change);
        }
        notebook
    }

    fn apply_change(mut self, change: Change) -> Self {
        use Change::*;
        match change {
            DeleteCell(DeleteCellChange { cell_id }) => self.with_updated_cells(|cells| {
                cells.retain(|cell| *cell.id() != cell_id);
            }),
            InsertCell(InsertCellChange { cell, index }) => self.with_updated_cells(|cells| {
                cells.insert(index as usize, cell);
            }),
            MoveCells(MoveCellsChange { cell_ids, index }) => self.with_updated_cells(|cells| {
                cell_ids.iter().enumerate().for_each(|(i, cell_id)| {
                    if let Some(old_index) = cells.iter().position(|c| c.id() == cell_id) {
                        let cell = cells.remove(old_index);
                        let new_index = index as usize + i;
                        cells.insert(new_index, cell);
                    }
                });
            }),
            UpdateCell(UpdateCellChange { cell }) => self.with_updated_cells(|cells| {
                if let Some(index) = cells.iter().position(|c| c.id() == cell.id()) {
                    cells[index] = cell;
                }
            }),
            UpdateCellText(UpdateCellTextChange {
                cell_id,
                text,
                formatting,
            }) => self.with_updated_cells(|cells| {
                if let Some(index) = cells.iter().position(|c| c.id() == &cell_id) {
                    cells[index] =
                        cells[index].with_rich_text(&text, formatting.unwrap_or_default());
                }
            }),
            UpdateNotebookTimeRange(UpdateNotebookTimeRangeChange { time_range }) => Self {
                0: Notebook {
                    time_range,
                    ..self.0
                },
            },
            UpdateNotebookTitle(UpdateNotebookTitleChange { title }) => Self {
                0: Notebook { title, ..self.0 },
            },
            AddDataSource(change) => {
                self.0.data_sources.insert(change.name, *change.data_source);
                self
            }
            UpdateDataSource(change) => {
                self.0.data_sources.insert(change.name, *change.data_source);
                self
            }
            DeleteDataSource(change) => {
                self.0.data_sources.remove(&change.name);
                self
            }
            AddLabel(change) => {
                self.0.labels.push(change.label);
                self
            }
            ReplaceLabel(change) => {
                if let Some(label) = self
                    .0
                    .labels
                    .iter_mut()
                    .find(|label| label.key == change.key)
                {
                    *label = change.label
                };
                self
            }
            RemoveLabel(change) => {
                self.0.labels.retain(|label| *label.key != change.label.key);
                self
            }
        }
    }

    pub fn clone_cell_with_index_by_id(&self, id: &str) -> CellWithIndex {
        self.0
            .cells
            .iter()
            .enumerate()
            .find(|(_, cell)| cell.id() == id)
            .map(|(index, cell)| CellWithIndex {
                cell: cell.clone(),
                index: index as u32,
            })
            .expect("No cell found with that ID")
    }

    /// Returns the notebook state with all the cells necessary for applying the given operation
    /// to it.
    fn state_for_operation(&self, operation: &Operation) -> NotebookState {
        let cell_ids = relevant_cell_ids_for_operation(operation);
        NotebookState {
            cells: self
                .0
                .cells
                .iter()
                .enumerate()
                .filter(|(_, cell)| cell_ids.contains(cell.id()))
                .map(|(index, cell)| CellRefWithIndex {
                    cell,
                    index: index as u32,
                })
                .collect(),
        }
    }

    /// Returns a copy of the notebook with updated cells.
    pub fn with_updated_cells<F>(&self, updater: F) -> Self
    where
        F: FnOnce(&mut Vec<Cell>),
    {
        let mut clone = self.clone();
        updater(&mut clone.0.cells);
        clone
    }

    /// Returns a copy of the notebook with updated cells.
    pub fn with_updated_data_sources<F>(&self, updater: F) -> Self
    where
        F: FnOnce(&mut BTreeMap<String, NotebookDataSource>),
    {
        let mut clone = self.clone();
        updater(&mut clone.0.data_sources);
        clone
    }
}

impl TransformOperationState for MemoryNotebook {
    fn cell(&self, id: &str) -> Option<&Cell> {
        self.0.cells.iter().find(|cell| cell.id() == id)
    }

    fn cell_index(&self, id: &str) -> Option<u32> {
        self.0
            .cells
            .iter()
            .position(|cell| cell.id() == id)
            .map(|index| index as u32)
    }
}

struct NotebookState<'a> {
    cells: Vec<CellRefWithIndex<'a>>,
}

// Note: It would be easier to just use the trait implementation of `Notebook`, but the reason I'm
// still sticking with a separate struct is so that we test `relevant_cell_ids_for_operation()` in
// the process.
impl<'a> ApplyOperationState for NotebookState<'a> {
    fn all_relevant_cells(&self) -> Vec<CellRefWithIndex> {
        self.cells.clone()
    }
}

struct Inner {
    notebook: MemoryNotebook,
    reply_waiters: HashMap<String, oneshot::Sender<ServerRealtimeMessage>>,
    operations_queue: HashMap<String, Operation>,
}

pub struct Worker {
    inner: Arc<RwLock<Inner>>,
    queue: tokio::sync::mpsc::Sender<ClientRealtimeMessage>,
    notebook_id: String,
}

impl Worker {
    pub async fn new(url: String, notebook_id: String, config: Option<String>) -> Result<Worker> {
        let client_cfg = api_client_configuration(config.as_deref(), &url)
            .await
            .unwrap();

        debug!(?client_cfg);

        let nb = fp_api_client::apis::default_api::get_notebook(&client_cfg, &notebook_id)
            .await
            .map_err(|e| anyhow!("Notebook with id `{}` not found: {:?}", notebook_id, e))?;

        let inb = MemoryNotebook(Notebook {
            cells: nb.cells.into_iter().map(cell_mapper).collect(),
            id: nb.id,
            data_sources: Default::default(),
            read_only: false,
            revision: nb.revision as u32,
            time_range: TimeRange {
                from: nb.time_range.from.into(),
                to: nb.time_range.to.into(),
            },
            title: nb.title,
            visibility: fiberplane::operations::NotebookVisibility::Public,
            created_at: OffsetDateTime::parse(
                &nb.created_at,
                &time::format_description::well_known::Rfc3339,
            )?,
            updated_at: OffsetDateTime::parse(
                &nb.updated_at,
                &time::format_description::well_known::Rfc3339,
            )?,
            created_by: CreatedBy {
                name: nb.created_by.name,
                user_type: UserType::Individual, //whatever
            },
            labels: vec![],
        });

        debug!(?inb, ?client_cfg, "Retrieved initial notebook");

        let (tx, rx) = tokio::sync::mpsc::channel(128);
        let this = Self {
            inner: Arc::new(RwLock::new(Inner {
                notebook: inb,
                reply_waiters: HashMap::new(),
                operations_queue: HashMap::new(),
            })),
            queue: tx,
            notebook_id: notebook_id.clone(),
        };

        let mut url = url::Url::parse(&url)?;

        url.set_scheme(if url.scheme() == "https" { "wss" } else { "ws" })
            .unwrap();
        url.set_path("/api/ws");

        let bearer = client_cfg
            .bearer_access_token
            .ok_or_else(|| anyhow!("missing bearer token"))?;

        let (ws_stream, _) = connect_async(url)
            .await
            .map_err(|e| anyhow!("unable to connect to web socket server: {:?}", e))?;

        info!("Connected client");

        let (sink, stream) = ws_stream.split();

        tokio::spawn(Self::spawn_reader(this.inner.clone(), stream));
        tokio::spawn(Self::spawn_writer(sink, rx));

        // First message must be Authenticate.
        let message = realtime::AuthenticateMessage {
            op_id: Some("auth".into()),
            token: bearer,
        };
        let message = realtime::ClientRealtimeMessage::Authenticate(message);

        this.send_message(message).await?;
        info!("Authenticated client");

        let message = realtime::SubscribeMessage {
            op_id: Some(format!("sub_{}", notebook_id)),
            notebook_id: notebook_id,
            revision: None,
        };
        let message = realtime::ClientRealtimeMessage::Subscribe(message);
        this.send_message(message).await?;
        info!("Subscribed client");

        info!("Client creation done");

        Ok(this)
    }

    pub fn get_random_cell(&self) -> Option<Cell> {
        self.inner
            .read()
            .unwrap()
            .notebook
            .0
            .cells
            .choose(&mut rand::thread_rng())
            .cloned()
    }

    pub async fn insert_cell(&self, cell: Cell) -> Result<String> {
        loop {
            let cell_id = Uuid::new_v4().to_string();
            let op_id = Uuid::new_v4().to_string();
            let (revision, len) = {
                let inner = self.inner.read().unwrap();
                (
                    inner.notebook.0.revision + 1,
                    inner.notebook.0.cells.len().saturating_sub(1),
                )
            };

            let operation = Operation::AddCells(AddCellsOperation {
                cells: vec![CellWithIndex {
                    cell: cell.with_id(&cell_id),
                    index: len as u32,
                }],
                referencing_cells: None,
            });

            {
                let mut inner = self.inner.write().unwrap();
                if let Ok(nb) = inner.notebook.apply_operation(&operation) {
                    inner.notebook = nb;
                    inner.notebook.0.revision = revision;
                } else {
                    break Err(anyhow!("Failed to apply operation locally"));
                }
            }

            let res = self
                .send_message(ClientRealtimeMessage::ApplyOperation(Box::new(
                    ApplyOperationMessage {
                        notebook_id: self.notebook_id.clone(),
                        operation: operation.clone(),
                        revision,
                        op_id: Some(op_id.clone()),
                    },
                )))
                .await;

            debug!(?res, "insert result");

            if let Ok(msg) = res {
                match msg {
                    ServerRealtimeMessage::Ack(_) => break Ok(cell_id),
                    _ => {
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    }
                }
            }
        }
    }

    pub async fn insert_text_cell(&self, content: String) -> Result<String> {
        self.insert_cell(Cell::Text(TextCell {
            id: Uuid::new_v4().to_string(),
            content: content.clone(),
            read_only: Some(false),
            formatting: None,
        }))
        .await
    }

    pub async fn write_text_cell(
        &self,
        content: String,
        character_delay: Option<Duration>,
    ) -> Result<()> {
        let cell_id = self.insert_text_cell(String::new()).await?;

        for c in content.chars() {
            let revision = self.get_next_revision();
            let cell = Box::new(self.get_cell(&cell_id).unwrap());
            let operation = Operation::UpdateCell(UpdateCellOperation {
                updated_cell: Box::new(cell.with_appended_content(&c.to_string())),
                old_cell: cell,
            });

            let _res = self
                .send_message(ClientRealtimeMessage::ApplyOperation(Box::new(
                    ApplyOperationMessage {
                        notebook_id: self.notebook_id.clone(),
                        operation,
                        revision,
                        op_id: Some(Uuid::new_v4().to_string()),
                    },
                )))
                .await;

            if let Some(dur) = &character_delay {
                tokio::time::sleep(dur.clone()).await;
            }
        }

        Ok(())
    }

    pub async fn replace_cell_sections_batch(
        &self,
        cell_id: &str,
        sections: Vec<(Range<usize>, String)>,
    ) -> Result<()> {
        let cell = self
            .get_cell(cell_id)
            .ok_or_else(|| anyhow!("cell not found"))?;
        let content = cell
            .content()
            .ok_or_else(|| anyhow!("cell without content"))?;

        self.send_operations(ApplyOperationBatchMessage {
            notebook_id: self.notebook_id.clone(),
            operations: sections
                .into_iter()
                .map(|(section, new_text)| {
                    let clamped = std::cmp::min(section.start, content.len())
                        ..std::cmp::min(section.end, content.len());
                    let old_text = content
                        .get(clamped.clone())
                        .ok_or_else(|| {
                            let good_start = {
                                let mut i = clamped.start;
                                loop {
                                    if content.is_char_boundary(i) {
                                        break i;
                                    }
                                    i -= 1;
                                }
                            };
                            let good_end = {
                                let mut i2 = clamped.end;
                                loop {
                                    if content.is_char_boundary(i2) {
                                        break i2;
                                    }
                                    i2 += 1;
                                }
                            };
                            anyhow!(
                                "section [{:?}] outside content len {}: >>>{}<<<",
                                clamped,
                                content.len(),
                                content[good_start..good_end].escape_debug()
                            )
                        })
                        .unwrap()
                        .to_string();

                    Operation::ReplaceText(ReplaceTextOperation {
                        cell_id: cell_id.to_string(),
                        offset: section.start as u32,
                        new_text,
                        new_formatting: None,
                        old_text,
                        old_formatting: None,
                    })
                })
                .collect(),
            revision: self.get_next_revision(),
            op_id: Some(Uuid::new_v4().to_string()),
        })
        .await?;

        Ok(())
    }

    pub async fn replace_cell_content(
        &self,
        cell_id: &str,
        new_content: &str,
        offset: u32,
    ) -> Result<()> {
        let cell = self
            .get_cell(cell_id)
            .ok_or_else(|| anyhow!("cell not found"))?;
        let content = cell
            .content()
            .ok_or_else(|| anyhow!("cell without content"))?;

        let offset_u = (offset as usize);

        let content = if offset_u < content.len() {
            &content[offset_u..]
        } else {
            content
        };

        if content == new_content {
            return Ok(());
        }

        self.send_operation(ApplyOperationMessage {
            notebook_id: self.notebook_id.clone(),
            operation: Operation::ReplaceText(ReplaceTextOperation {
                cell_id: cell_id.to_string(),
                offset,
                new_text: new_content.to_string(),
                new_formatting: None,
                old_text: content.to_string(),
                old_formatting: None,
            }),
            revision: self.get_next_revision(),
            op_id: Some(Uuid::new_v4().to_string()),
        })
        .await?;

        Ok(())
    }

    pub async fn append_cell_content(&self, cell_id: &str, content: &str) -> Result<()> {
        let cell = self
            .get_cell(cell_id)
            .ok_or_else(|| anyhow!("cell not found"))?;
        let char_len = cell
            .content()
            .map(|s| s.len())
            .ok_or_else(|| anyhow!("cell without content"))?;
        let offset = char_len as u32;

        self.send_operation(ApplyOperationMessage {
            notebook_id: self.notebook_id.clone(),
            operation: Operation::ReplaceText(ReplaceTextOperation {
                cell_id: cell_id.to_string(),
                offset,
                new_text: content.to_string(),
                new_formatting: None,
                old_text: String::default(),
                old_formatting: None,
            }),
            revision: self.get_next_revision(),
            op_id: Some(Uuid::new_v4().to_string()),
        })
        .await?;

        Ok(())
    }

    pub async fn remove_cell_content(&self, cell_id: &str, section: Range<usize>) -> Result<()> {
        let cell = self
            .get_cell(cell_id)
            .ok_or_else(|| anyhow!("cell not found"))?;
        let content = cell
            .content()
            .ok_or_else(|| anyhow!("cell without content"))?;

        let old_text = content
            .get(section.clone())
            .ok_or_else(|| anyhow!("section outside content range"))?
            .to_string();

        self.send_operation(ApplyOperationMessage {
            notebook_id: self.notebook_id.clone(),
            operation: Operation::ReplaceText(ReplaceTextOperation {
                cell_id: cell_id.to_string(),
                offset: section.start as u32,
                new_text: String::default(),
                new_formatting: None,
                old_text,
                old_formatting: None,
            }),
            revision: self.get_next_revision(),
            op_id: Some(Uuid::new_v4().to_string()),
        })
        .await?;

        Ok(())
    }

    pub async fn send_operation(
        &self,
        mut message: ApplyOperationMessage,
    ) -> Result<ServerRealtimeMessage> {
        for _ in 1..=3 {
            match self
                .send_message(ClientRealtimeMessage::ApplyOperation(Box::new(
                    message.clone(),
                )))
                .await
            {
                Ok(ServerRealtimeMessage::Rejected(r)) => {
                    message.revision = self.get_next_revision();
                    message.op_id = Some(Uuid::new_v4().to_string());
                    continue;
                }
                o => return o,
            }
        }

        Err(anyhow!("Failed to send operation after 3 retries"))
    }
    pub async fn send_operations(
        &self,
        mut message: ApplyOperationBatchMessage,
    ) -> Result<ServerRealtimeMessage> {
        let mut msg = None;
        for _ in 1..=3 {
            match self
                .send_message(ClientRealtimeMessage::ApplyOperationBatch(Box::new(
                    message.clone(),
                )))
                .await
            {
                Ok(ServerRealtimeMessage::Rejected(r)) => {
                    msg = Some(r);
                    message.revision = self.get_next_revision();
                    message.op_id = Some(Uuid::new_v4().to_string());
                    continue;
                }
                o => return o,
            }
        }

        Err(anyhow!(
            "Failed to send operation after 3 retries: {:?}",
            msg
        ))
    }

    async fn send_message(&self, message: ClientRealtimeMessage) -> Result<ServerRealtimeMessage> {
        info!(?message, "Queueing message");
        let (tx, rx) = oneshot::channel();

        {
            let mut inner = self.inner.write().unwrap();
            let k = message.op_id().clone().unwrap();

            inner.reply_waiters.insert(k, tx);
        }

        self.queue.send(message).await?;
        info!("Queueing done, awaiting response");
        let res = rx.await.map_err(|e| anyhow!("oneshot error: {}", e));

        info!(?res, "Got response");
        res
    }

    async fn spawn_writer(
        mut sink: SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>,
        mut chan: tokio::sync::mpsc::Receiver<ClientRealtimeMessage>,
    ) {
        while let Some(msg) = chan.recv().await {
            info!(?msg, "writer loop");
            let data = match serde_json::to_string(&msg) {
                Ok(data) => data,
                Err(e) => {
                    error!(?e, ?msg, "writer serde error");
                    continue;
                }
            };
            debug!(?data, "Sending message");
            if let Err(e) = sink.send(Message::Text(data)).await {
                error!(?e, "got write error");
            }
        }
        warn!("Exiting writer loop");
    }

    async fn spawn_reader(
        inner: Arc<RwLock<Inner>>,
        mut stream: SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>,
    ) {
        while let Some(message) = stream.next().await {
            let message = match message {
                Ok(m) => m,
                Err(e) => {
                    error!(?e, "Got read error");
                    continue;
                }
            };
            match message {
                Message::Text(message) => {
                    debug!(%message, "Received message");

                    let msg = if let Ok(msg) = serde_json::from_slice(message.as_bytes()) {
                        msg
                    } else {
                        warn!("Failed to serde read message");
                        continue;
                    };

                    info!(?msg, "Deserialized");

                    let inner = &mut inner.write().unwrap();

                    match &msg {
                        ServerRealtimeMessage::ApplyOperation(op) => {
                            if let Ok(nb) = inner.notebook.apply_operation(&op.operation) {
                                inner.notebook = nb;
                                inner.notebook.0.revision = op.revision;
                            }
                        }
                        ServerRealtimeMessage::Rejected(r) => {
                            debug!("reverting rejected operation");
                        }
                        _ => {}
                    }

                    get_op_id(&msg)
                        .and_then(|id| inner.reply_waiters.remove(&id))
                        .map(|tx| tx.send(msg).unwrap());
                }
                Message::Binary(_) => debug!("Received unexpected binary content"),
                Message::Ping(_) => debug!("Received ping message"),
                Message::Pong(_) => debug!("Received pong message"),
                Message::Close(_) => debug!("Received close message"),
                Message::Frame(_) => debug!("Received frame message"),
            };
        }

        warn!("Reader loop exited");
    }

    fn get_cell(&self, id: impl AsRef<str>) -> Option<Cell> {
        self.inner
            .read()
            .unwrap()
            .notebook
            .0
            .cell(id.as_ref())
            .cloned()
    }

    fn get_revision(&self) -> u32 {
        self.inner.read().unwrap().notebook.0.revision
    }
    fn get_next_revision(&self) -> u32 {
        self.get_revision() + 1
    }
}

fn annotiation_mapper(annotation: fp_api_client::models::Annotation) -> AnnotationWithOffset {
    use fp_api_client::models::Annotation::*;
    let (annotation, offset) = match annotation {
        EndBoldAnnotation { offset } => (Annotation::EndBold, offset),
        EndHighlightAnnotation { offset } => (Annotation::EndHighlight, offset),
        EndItalicsAnnotation { offset } => (Annotation::EndItalics, offset),
        EndLinkAnnotation { offset } => (Annotation::EndLink, offset),
        EndStrikethroughAnnotation { offset } => (Annotation::EndStrikethrough, offset),
        EndUnderlineAnnotation { offset } => (Annotation::EndUnderline, offset),
        MentionAnnotation {
            offset,
            name,
            user_id,
        } => (Annotation::Mention(Mention { name, user_id }), offset),
        StartBoldAnnotation { offset } => (Annotation::StartBold, offset),
        StartHighlightAnnotation { offset } => (Annotation::StartHighlight, offset),
        StartItalicsAnnotation { offset } => (Annotation::StartItalics, offset),
        StartLinkAnnotation { offset, url } => (Annotation::StartLink { url }, offset),
        StartStrikethroughAnnotation { offset } => (Annotation::StartStrikethrough, offset),
        StartUnderlineAnnotation { offset } => (Annotation::StartUnderline, offset),
    };
    AnnotationWithOffset {
        annotation,
        offset: offset as u32,
    }
}

fn cell_mapper(cell: fp_api_client::models::Cell) -> Cell {
    match cell {
        fp_api_client::models::Cell::CheckboxCell {
            id,
            checked,
            content,
            level,
            read_only,
            formatting,
        } => Cell::Checkbox(CheckboxCell {
            id,
            checked,
            content,
            level: level.map(|l| l as u8),
            read_only,
            formatting: formatting.map(|f| f.into_iter().map(annotiation_mapper).collect()),
        }),
        fp_api_client::models::Cell::CodeCell {
            id,
            content,
            read_only,
            syntax,
        } => Cell::Code(CodeCell {
            id,
            content,
            read_only,
            syntax,
        }),
        fp_api_client::models::Cell::GraphCell {
            id,
            graph_type,
            stacking_type,
            read_only,
            source_ids,
            time_range,
            title,
            data,
            formatting,
        } => todo!(),
        fp_api_client::models::Cell::HeadingCell {
            id,
            heading_type,
            content,
            read_only,
            formatting,
        } => todo!(),
        fp_api_client::models::Cell::ImageCell {
            id,
            file_id,
            progress,
            read_only,
            width,
            height,
            preview,
            url,
        } => todo!(),
        fp_api_client::models::Cell::ListItemCell {
            id,
            list_type,
            content,
            level,
            read_only,
            formatting,
            start_number,
        } => todo!(),
        fp_api_client::models::Cell::PrometheusCell {
            id,
            content,
            read_only,
        } => todo!(),
        fp_api_client::models::Cell::TableCell {
            id,
            read_only,
            source_ids,
            data,
        } => todo!(),
        fp_api_client::models::Cell::TextCell {
            id,
            content,
            read_only,
            formatting,
        } => Cell::Text(TextCell {
            id,
            content,
            read_only,
            formatting: formatting.map(|f| f.into_iter().map(annotiation_mapper).collect()),
        }),
        fp_api_client::models::Cell::DividerCell { id, read_only } => todo!(),
        fp_api_client::models::Cell::ElasticsearchCell {
            id,
            content,
            read_only,
        } => todo!(),
        fp_api_client::models::Cell::LogCell {
            id,
            read_only,
            source_ids,
            time_range,
            data,
        } => todo!(),
        fp_api_client::models::Cell::LokiCell {
            id,
            content,
            read_only,
        } => todo!(),
    }
}

fn get_op_id(msg: &ServerRealtimeMessage) -> Option<String> {
    match msg {
        ServerRealtimeMessage::ApplyOperation(o) => o.op_id.clone(),
        ServerRealtimeMessage::Ack(o) => Some(o.op_id.clone()),
        ServerRealtimeMessage::Err(o) => o.op_id.clone(),
        ServerRealtimeMessage::DebugResponse(o) => o.op_id.clone(),
        ServerRealtimeMessage::Rejected(o) => o.op_id.clone(),
        ServerRealtimeMessage::SubscriberAdded(_) => None,
        ServerRealtimeMessage::SubscriberRemoved(_) => None,
        ServerRealtimeMessage::SubscriberChangedFocus(_) => None,
        ServerRealtimeMessage::Mention(o) => None,
    }
}
