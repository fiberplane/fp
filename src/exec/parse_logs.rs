use fp_api_client::models::LogRecord;
use grok::{Grok, Pattern};
use once_cell::unsync::Lazy;
use serde::{de::Error, Deserialize, Deserializer};
use serde_json::{Map, Value};
use std::collections::HashMap;
use time::format_description::{well_known::Rfc3339, FormatItem};
use time::{macros::format_description, OffsetDateTime};
use tracing::warn;

pub(crate) static TIMESTAMP_FIELDS: &[&str] = &["@timestamp", "timestamp", "fields.timestamp"];
pub(crate) static BODY_FIELDS: &[&str] =
    &["body", "message", "fields.body", "fields.message", "log"];
// This mapping is based on the recommended mapping from the
// Elastic Common Schema to the OpenTelemetry Log specification
// https://github.com/open-telemetry/opentelemetry-specification/blob/main/specification/logs/data-model.md#elastic-common-schema
static RESOURCE_FIELD_PREFIXES: &[&str] = &["agent.", "cloud.", "container.", "host.", "service."];
static RESOURCE_FIELD_EXCEPTIONS: &[&str] = &["container.labels", "host.uptime", "service.state"];

static NGINX_GROK: &str = r#"%{IPORHOST:clientip} %{USER:ident} %{USER:auth} \[%{HTTPDATE:timestamp}\] "(?:%{WORD:verb} %{NOTSPACE:request}(?: HTTP/%{NUMBER:httpversion})?|%{DATA:rawrequest})" %{NUMBER:response} (?:%{NUMBER:bytes}|-) %{QS:referrer} %{QS:agent}"#;
const NGINX_TIMESTAMP_FORMAT: &[FormatItem] = format_description!("[day]/[month repr:short]/[year]:[hour repr:24]:[minute]:[second] [offset_hour sign:mandatory][offset_minute]");

#[derive(Deserialize, Debug)]
#[serde(untagged)]
enum AnyTimestamp {
    #[serde(with = "time::serde::timestamp")]
    Unix(OffsetDateTime),
    #[serde(with = "time::serde::rfc3339")]
    Rfc3339(OffsetDateTime),
    #[serde(with = "time::serde::iso8601")]
    Iso8601(OffsetDateTime),
    #[serde(with = "time::serde::rfc2822")]
    Rfc2822(OffsetDateTime),
    #[serde(deserialize_with = "deserialize_nginx_timestamp")]
    Nginx(OffsetDateTime),
}

pub fn deserialize_nginx_timestamp<'de, D>(deserializer: D) -> Result<OffsetDateTime, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    OffsetDateTime::parse(&s, NGINX_TIMESTAMP_FORMAT).map_err(D::Error::custom)
}

impl From<AnyTimestamp> for OffsetDateTime {
    fn from(timestamp: AnyTimestamp) -> Self {
        match timestamp {
            AnyTimestamp::Unix(t) => t,
            AnyTimestamp::Rfc3339(t) => t,
            AnyTimestamp::Iso8601(t) => t,
            AnyTimestamp::Rfc2822(t) => t,
            AnyTimestamp::Nginx(t) => t,
        }
    }
}

pub fn parse_logs(output: &str) -> HashMap<String, Vec<LogRecord>> {
    let mut logs: HashMap<String, Vec<LogRecord>> = HashMap::new();

    let nginx: Lazy<Pattern> = Lazy::new(|| Grok::default().compile(NGINX_GROK, false).unwrap());

    for line in output.lines() {
        let result = if let Ok(Value::Object(json)) = serde_json::from_str(line) {
            parse_json(json)
        } else if let Some(matches) = nginx.match_against(line) {
            let fields = matches
                .into_iter()
                .filter(|(k, _)| k.chars().all(|c| c.is_lowercase()))
                .map(|(k, v)| (k.to_string(), v.trim_matches('"').to_string()))
                .collect();
            parse_flattened_json(fields)
        } else {
            warn!("Unable to parse log line: {}", line);
            continue;
        };

        if let Some((timestamp, log)) = result {
            if let Some(records) = logs.get_mut(&timestamp) {
                records.push(log);
            } else {
                logs.insert(timestamp, vec![log]);
            }
        }
    }
    logs
}

fn parse_json(json: Map<String, Value>) -> Option<(String, LogRecord)> {
    let mut flattened_fields = HashMap::new();
    for (key, val) in json.into_iter() {
        flatten_nested_value(&mut flattened_fields, key, val);
    }
    parse_flattened_json(flattened_fields)
}

fn parse_flattened_json(
    mut flattened_fields: HashMap<String, String>,
) -> Option<(String, LogRecord)> {
    let trace_id = flattened_fields
        .remove("trace_id")
        .or(flattened_fields.remove("trace.id"));
    let span_id = flattened_fields
        .remove("span_id")
        .or(flattened_fields.remove("span.id"));

    // Find the timestamp field (or set it to NaN if none is found)
    // Note: this will leave the original timestamp field in the flattened_fields
    let mut timestamp: Option<OffsetDateTime> = None;
    for field_name in TIMESTAMP_FIELDS {
        if let Some(ts) = flattened_fields.remove(*field_name) {
            if let Ok(ts) = serde_json::from_value::<AnyTimestamp>(Value::String(ts)) {
                timestamp = Some(ts.into());
                break;
            }
        }
    }
    let timestamp_float = if let Some(timestamp) = timestamp {
        // TODO don't panic if this conversion fails
        timestamp.unix_timestamp() as f32
    } else {
        f32::NAN
    };

    // Find the body field (or set it to an empty string if none is found)
    // Note: this will leave the body field in the flattened_fields and copy
    // it into the body of the LogRecord
    let mut body = String::new();
    for field_name in BODY_FIELDS {
        if let Some(b) = flattened_fields.get(*field_name) {
            body = b.to_string();
            break;
        }
    }

    // All fields that are not mapped to the resource field
    // become part of the attributes field
    // TODO refactor this so we only make one pass over the fields
    let (resource, attributes): (HashMap<String, String>, HashMap<String, String>) =
        flattened_fields.into_iter().partition(|(key, _)| {
            RESOURCE_FIELD_PREFIXES
                .iter()
                .any(|prefix| key.starts_with(prefix))
                && !RESOURCE_FIELD_EXCEPTIONS.contains(&key.as_str())
        });

    // TODO can we do something better than ignoring lines without timestamps?
    timestamp.map(|timestamp| {
        (
            timestamp.format(&Rfc3339).unwrap(),
            LogRecord {
                trace_id,
                span_id,
                timestamp: timestamp_float,
                body,
                resource,
                attributes,
            },
        )
    })
}

fn flatten_nested_value(output: &mut HashMap<String, String>, key: String, value: Value) {
    match value {
        Value::Object(v) => {
            for (sub_key, val) in v.into_iter() {
                flatten_nested_value(output, format!("{}.{}", key, sub_key), val);
            }
        }
        Value::Array(v) => {
            for (index, val) in v.into_iter().enumerate() {
                // TODO should the separator be dots instead?
                flatten_nested_value(output, format!("{}[{}]", key, index), val);
            }
        }
        Value::String(v) => {
            output.insert(key, v);
        }
        Value::Number(v) => {
            output.insert(key, v.to_string());
        }
        Value::Bool(v) => {
            output.insert(key, v.to_string());
        }
        Value::Null => {
            output.insert(key, "".to_string());
        }
    };
}
