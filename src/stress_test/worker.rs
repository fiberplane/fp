use anyhow::{anyhow, Result};
use fiberplane::{
    operations::*,
    protocols::{
        core::*,
        operations::*,
        realtime::{self, ApplyOperationMessage, ClientRealtimeMessage, ServerRealtimeMessage},
    },
};
use futures_util::{
    stream::{SplitSink, SplitStream},
    SinkExt, StreamExt,
};
use rand::prelude::*;
use std::{collections::BTreeMap, time::Duration};
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};
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

        let nb = fiberplane_api::apis::default_api::get_notebook(&client_cfg, &notebook_id)
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
        });

        debug!(?inb, ?client_cfg, "Retrieved initial notebook");

        let (tx, rx) = tokio::sync::mpsc::channel(128);
        let this = Self {
            inner: Arc::new(RwLock::new(Inner {
                notebook: inb,
                reply_waiters: HashMap::new(),
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

    pub async fn insert_text_cell(&self, content: String) {
        //let cell = self.get_random_cell();

        loop {
            let id = Uuid::new_v4().to_string();
            let op_id = Uuid::new_v4().to_string();
            let (revision, len) = {
                let inner = self.inner.read().unwrap();
                (inner.notebook.0.revision, inner.notebook.0.cells.len())
            };

            let res = self
                .send_message(ClientRealtimeMessage::ApplyOperation(Box::new(
                    ApplyOperationMessage {
                        notebook_id: self.notebook_id.clone(),
                        operation: Operation::AddCells(AddCellsOperation {
                            cells: vec![CellWithIndex {
                                cell: Cell::Text(TextCell {
                                    id: id.clone(),
                                    content: content.clone(),
                                    read_only: Some(false),
                                }),
                                index: len as u32,
                            }],
                            referencing_cells: None,
                        }),
                        revision,
                        op_id: Some(op_id.clone()),
                    },
                )))
                .await;

            debug!(?res, "insert result");

            if let Ok(msg) = res {
                match msg {
                    ServerRealtimeMessage::Ack(_) => break,
                    _ => {
                        tokio::time::sleep(Duration::from_millis(200)).await;
                    }
                }
            }
        }
    }

    async fn send_message(&self, message: ClientRealtimeMessage) -> Result<ServerRealtimeMessage> {
        info!(?message, "Queueing message");
        let (tx, rx) = oneshot::channel();

        self.inner
            .write()
            .unwrap()
            .reply_waiters
            .insert(message.op_id().clone().unwrap(), tx);

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

                    if let ServerRealtimeMessage::ApplyOperation(op) = &msg {
                        if let Ok(nb) = inner.notebook.apply_operation(&op.operation) {
                            inner.notebook = nb;
                        }
                    }

                    get_op_id(&msg)
                        .and_then(|id| inner.reply_waiters.remove(&id))
                        .map(|tx| tx.send(msg).unwrap());
                }
                Message::Binary(_) => eprintln!("Received unexpected binary content"),
                Message::Ping(_) => eprintln!("Received ping message"),
                Message::Pong(_) => eprintln!("Received pong message"),
                Message::Close(_) => eprintln!("Received close message"),
            };
        }

        warn!("Reader loop exited");
    }
}

fn cell_mapper(cell: fiberplane_api::models::Cell) -> Cell {
    match cell {
        fiberplane_api::models::Cell::CheckboxCell {
            id,
            checked,
            content,
            level,
            read_only,
        } => Cell::Checkbox(CheckboxCell {
            id,
            checked,
            content,
            level: level.map(|l| l as u8),
            read_only,
        }),
        fiberplane_api::models::Cell::CodeCell {
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
        fiberplane_api::models::Cell::GraphCell {
            id,
            graph_type,
            stacking_type,
            read_only,
            source_ids,
            time_range,
            title,
            data,
        } => todo!(),
        fiberplane_api::models::Cell::HeadingCell {
            id,
            heading_type,
            content,
            read_only,
        } => todo!(),
        fiberplane_api::models::Cell::ImageCell {
            id,
            file_id,
            progress,
            read_only,
            width,
            height,
            preview,
        } => todo!(),
        fiberplane_api::models::Cell::ListItemCell {
            id,
            list_type,
            content,
            level,
            read_only,
        } => todo!(),
        fiberplane_api::models::Cell::PrometheusCell {
            id,
            content,
            read_only,
        } => todo!(),
        fiberplane_api::models::Cell::TableCell {
            id,
            read_only,
            source_ids,
            data,
        } => todo!(),
        fiberplane_api::models::Cell::TextCell {
            id,
            content,
            read_only,
        } => Cell::Text(TextCell {
            id,
            content,
            read_only,
        }),
    }
}

fn get_op_id(msg: &ServerRealtimeMessage) -> Option<String> {
    match msg {
        ServerRealtimeMessage::ApplyOperation(o) => o.op_id.clone(),
        ServerRealtimeMessage::Ack(o) => Some(o.op_id.clone()),
        ServerRealtimeMessage::Err(o) => o.op_id.clone(),
        ServerRealtimeMessage::DebugResponse(o) => o.op_id.clone(),
        ServerRealtimeMessage::Reject(o) => o.op_id.clone(),
        ServerRealtimeMessage::SubscriberAdded(_) => None,
        ServerRealtimeMessage::SubscriberRemoved(_) => None,
        ServerRealtimeMessage::SubscriberChangedFocus(_) => None,
    }
}
