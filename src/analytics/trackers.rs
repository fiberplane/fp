use crate::analytics::Analytics;
use gethostname::gethostname;
use segment::message::{BatchMessage, Track};
use serde_json::json;
use time::OffsetDateTime;

impl Analytics {
    pub(crate) async fn user_logged_in(&self) {
        self.queue(BatchMessage::Track(Track {
            user: self.user.clone(),
            event: "cli | login".to_string(),
            properties: json!({
                "os": std::env::consts::OS,
                "arch": std::env::consts::ARCH,
                "hostname": gethostname().into_string().ok()
            }),
            timestamp: Some(OffsetDateTime::now_utc()),
            ..Default::default()
        }))
        .await
    }

    pub(crate) async fn notebook_new(&self) {
        self.queue(BatchMessage::Track(Track {
            user: self.user.clone(),
            event: "cli | newNotebook".to_string(),
            properties: json!({
                "os": std::env::consts::OS,
                "arch": std::env::consts::ARCH,
                "hostname": gethostname().into_string().ok()
            }),
            timestamp: Some(OffsetDateTime::now_utc()),
            ..Default::default()
        }))
        .await
    }
}
