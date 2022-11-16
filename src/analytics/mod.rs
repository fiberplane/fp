use crate::config::Config;
use crate::Arguments;
use anyhow::Result;
use segment::message::{BatchMessage, User};
use segment::{AutoBatcher, Batcher, HttpClient};
use tokio::sync::Mutex;

pub(crate) mod command;
pub(crate) mod trackers;

static SEGMENT_WRITE_KEY: &str = "XoSHALxJJEBTJKzc2o6hjDB2XZKAwp1c";

pub struct Analytics {
    // RwLock makes no sense here as we don't ever read actually, but we modify from different threads
    batcher: Option<Mutex<AutoBatcher>>,
    pub(crate) user: User,
}

impl Analytics {
    pub(crate) async fn new(args: &Arguments) -> Result<Self> {
        // make an empty struct here, so it can be filled by config values below in `.reload_config`
        let mut this = Self {
            batcher: None,
            user: User::AnonymousId {
                anonymous_id: "".to_string(),
            },
        };

        this.reload_config(args).await?;
        Ok(this)
    }

    pub(crate) async fn reload_config(&mut self, args: &Arguments) -> Result<()> {
        let config = Config::load(args.config.clone()).await?;

        self.user = match config.user_id {
            Some(user_id) => User::Both {
                user_id: user_id.to_string(),
                anonymous_id: config.anonymous_id.to_string(),
            },
            None => User::AnonymousId {
                anonymous_id: config.anonymous_id.to_string(),
            },
        };

        self.batcher = if config.analytics {
            Some(Mutex::new(AutoBatcher::new(
                HttpClient::default(),
                Batcher::new(None),
                SEGMENT_WRITE_KEY.to_string(),
            )))
        } else {
            None
        };

        Ok(())
    }

    pub(crate) async fn queue(&self, message: BatchMessage) {
        if let Some(batcher) = &self.batcher {
            let _ = batcher.lock().await.push(message).await;
        }
    }

    pub(crate) async fn flush(&self) -> segment::Result<()> {
        if let Some(batcher) = &self.batcher {
            batcher.lock().await.flush().await
        } else {
            Ok(())
        }
    }
}
