use super::timestamp::AnyTimestamp;
use fp_api_client::models::LogRecord;
use grok::{Grok, Pattern};
use once_cell::sync::Lazy;
use serde_json::{Map, Value};
use std::collections::HashMap;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use tracing::warn;

pub(crate) static TIMESTAMP_FIELDS: &[&str] =
    &["@timestamp", "timestamp", "fields.timestamp", "ts"];
pub(crate) static BODY_FIELDS: &[&str] = &[
    "body",
    "message",
    "fields.body",
    "fields.message",
    "log",
    "msg",
];
// This mapping is based on the recommended mapping from the
// Elastic Common Schema to the OpenTelemetry Log specification
// https://github.com/open-telemetry/opentelemetry-specification/blob/main/specification/logs/data-model.md#elastic-common-schema
static RESOURCE_FIELD_PREFIXES: &[&str] = &["agent.", "cloud.", "container.", "host.", "service."];
static RESOURCE_FIELD_EXCEPTIONS: &[&str] = &["container.labels", "host.uptime", "service.state"];

static NGINX_PATTERN: Lazy<Pattern> = Lazy::new(|| {
    let pattern = r#"%{IPORHOST:clientip} %{USER:ident} %{USER:auth} \[%{HTTPDATE:timestamp}\] "(?:%{WORD:verb} %{NOTSPACE:request}(?: HTTP/%{NUMBER:httpversion})?|%{DATA:rawrequest})" %{NUMBER:response} (?:%{NUMBER:bytes}|-) %{QS:referrer} %{QS:agent}"#;
    Grok::default().compile(pattern, true).unwrap()
});
static GITHUB_ACTION_PATTERN: Lazy<Pattern> = Lazy::new(|| {
    let pattern = r"%{WORD:job}%{SPACE}%{DATA:step}%{SPACE}%{TIMESTAMP_ISO8601:timestamp}%{SPACE}%{GREEDYDATA:body}";
    Grok::default().compile(pattern, true).unwrap()
});

/// Parse logs from each line of the string.
/// This handles JSON-encoded log lines as well as a variety of other log formats.
///
/// The HashMap returned is keyed by the timestamp of the log record (to handle multiple logs at the same instant).
pub fn parse_logs(output: &str) -> HashMap<String, Vec<LogRecord>> {
    let mut logs: HashMap<String, Vec<LogRecord>> = HashMap::new();
    // Keep track of the most recent timestamp in case later log lines do not have a timestamp
    let mut most_recent_timestamp = None;
    // Keep track of lines without timestamps so we can add them to a later entry with a timestamp
    let mut lines_without_timestamps = Vec::new();

    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        match parse_log(line) {
            Some((timestamp, log)) => {
                let entries_at_timestamp = logs.entry(timestamp.clone()).or_insert_with(Vec::new);

                // If we had lines before that didn't have timestamps, add them under this
                // timestamp (and put them first in the array)
                if !lines_without_timestamps.is_empty() {
                    *entries_at_timestamp = lines_without_timestamps
                        .split_off(0)
                        .into_iter()
                        .map(|line| LogRecord {
                            timestamp: log.timestamp,
                            body: line,
                            attributes: Default::default(),
                            resource: Default::default(),
                            trace_id: None,
                            span_id: None,
                        })
                        .chain(entries_at_timestamp.split_off(0))
                        .collect();
                }

                most_recent_timestamp = Some((timestamp, log.timestamp));
                entries_at_timestamp.push(log);
            }
            None => {
                if let Some((timestamp_string, timestamp)) = &most_recent_timestamp {
                    if let Some(logs) = logs.get_mut(timestamp_string) {
                        logs.push(LogRecord {
                            timestamp: *timestamp,
                            body: line.to_string(),
                            attributes: Default::default(),
                            resource: Default::default(),
                            trace_id: None,
                            span_id: None,
                        })
                    }
                }

                lines_without_timestamps.push(line.to_string());
            }
        }
    }

    // If none of the lines had timestamps, use the current moment as the timestamp
    if !lines_without_timestamps.is_empty() {
        let now = OffsetDateTime::now_utc();
        let timestamp = now.unix_timestamp() as f32;
        logs.entry(now.format(&Rfc3339).unwrap())
            .or_insert_with(Vec::new)
            .extend(lines_without_timestamps.into_iter().map(|line| LogRecord {
                timestamp,
                body: line,
                attributes: Default::default(),
                resource: Default::default(),
                trace_id: None,
                span_id: None,
            }));
    }

    logs
}

pub fn contains_logs(output: &str) -> bool {
    output
        .lines()
        .filter(|line| !line.trim().is_empty())
        .any(|line| parse_log(line).is_some())
}

fn parse_log(line: &str) -> Option<(String, LogRecord)> {
    if let Ok(Value::Object(json)) = serde_json::from_str(line) {
        parse_json(json)
    } else if let Some(matches) = NGINX_PATTERN
        .match_against(line)
        .or_else(|| GITHUB_ACTION_PATTERN.match_against(line))
    {
        let fields = matches
            .into_iter()
            // The keys written in upper case are the grok components used to build up the values we care about
            .filter(|(k, _)| k.chars().all(|c| c.is_lowercase()))
            .map(|(k, v)| (k.to_string(), v.trim_matches('"').to_string()))
            .collect();
        parse_flattened_json(fields)
    } else {
        None
    }
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
        .or_else(|| flattened_fields.remove("trace.id"));
    let span_id = flattened_fields
        .remove("span_id")
        .or_else(|| flattened_fields.remove("span.id"));

    // Find the timestamp field (or set it to NaN if none is found)
    // Note: this will leave the original timestamp field in the flattened_fields
    let mut timestamp: Option<OffsetDateTime> = None;
    for field_name in TIMESTAMP_FIELDS {
        if let Some(ts) = flattened_fields.remove(*field_name) {
            match serde_json::from_value::<AnyTimestamp>(Value::String(ts)) {
                Ok(ts) => {
                    timestamp = Some(ts.into());
                    break;
                }
                Err(err) => {
                    warn!("Unable to parse timestamp: {}", err);
                }
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
        if let Some(b) = flattened_fields.remove(*field_name) {
            body = b;
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::iter::FromIterator;

    #[test]
    fn json_logs() {
        let logs = r#"{"ts": "2018-01-01T00:00:00.000Z", "body": "test"}
        {"timestamp": "1657619253", "message": "hello", "trace_id": "123", "thing": 1, "host.name": "blah"}"#;
        let logs = parse_logs(logs);
        assert_eq!(logs.len(), 2);
        assert_eq!(logs["2018-01-01T00:00:00Z"][0].body, "test");
        assert_eq!(logs["2022-07-12T09:47:33Z"][0].body, "hello");
        assert_eq!(
            logs["2022-07-12T09:47:33Z"][0].trace_id,
            Some("123".to_string())
        );
        assert_eq!(logs["2022-07-12T09:47:33Z"][0].attributes["thing"], "1");
        assert_eq!(
            logs["2022-07-12T09:47:33Z"][0].resource["host.name"],
            "blah"
        );
    }

    #[test]
    fn nginx_logs() {
        let logs = r#"
192.0.7.128 - - [11/Jul/2022:13:04:26 +0000] "GET / HTTP/1.1" 200 472 "-" "ELB-HealthChecker/2.0" "-"
192.0.6.198 - - [11/Jul/2022:13:04:27 +0000] "GET / HTTP/1.1" 200 472 "-" "ELB-HealthChecker/2.0" "-""#;
        let logs = parse_logs(logs);
        assert_eq!(logs.len(), 2);
        let mut attributes = HashMap::from_iter(
            [
                ("auth", "-"),
                ("referrer", "-"),
                ("ident", "-"),
                ("clientip", "192.0.7.128"),
                ("verb", "GET"),
                ("agent", "ELB-HealthChecker/2.0"),
                ("response", "200"),
                ("bytes", "472"),
                ("httpversion", "1.1"),
                ("request", "/"),
            ]
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string())),
        );
        assert_eq!(logs["2022-07-11T13:04:26Z"][0].attributes, attributes);

        *attributes.get_mut("clientip").unwrap() = "192.0.6.198".to_string();
        assert_eq!(logs["2022-07-11T13:04:27Z"][0].attributes, attributes);
    }

    #[test]
    fn github_action_logs() {
        let logs = "
build   Set up job      2022-07-11T15:12:28.2324317Z Packages: write
build   Set up job      2022-07-11T15:12:28.2324660Z Pages: write
build   Set up job      2022-07-11T15:12:28.2325020Z PullRequests: write";
        let logs = parse_logs(logs);
        assert_eq!(logs.len(), 3);
        assert_eq!(
            logs["2022-07-11T15:12:28.2324317Z"][0].timestamp,
            1657552348.2324317
        );
        assert_eq!(
            logs["2022-07-11T15:12:28.2324317Z"][0].body,
            "Packages: write"
        );
        assert_eq!(
            logs["2022-07-11T15:12:28.2324317Z"][0].attributes["job"],
            "build"
        );
        assert_eq!(
            logs["2022-07-11T15:12:28.2324317Z"][0].attributes["step"],
            "Set up job"
        );
    }

    #[test]
    fn no_timestamps() {
        let logs = r#"
/docker-entrypoint.sh: Launching /docker-entrypoint.d/20-envsubst-on-templates.sh
/docker-entrypoint.sh: Launching /docker-entrypoint.d/30-tune-worker-processes.sh
/docker-entrypoint.sh: Configuration complete; ready for start up"#;
        let logs = parse_logs(logs);
        let (_, entries) = logs.into_iter().next().unwrap();
        assert_eq!(entries.len(), 3);
    }

    #[test]
    fn lines_without_timestamps() {
        let logs = r#"
/docker-entrypoint.sh: Launching /docker-entrypoint.d/20-envsubst-on-templates.sh
/docker-entrypoint.sh: Launching /docker-entrypoint.d/30-tune-worker-processes.sh
/docker-entrypoint.sh: Configuration complete; ready for start up
192.0.7.128 - - [11/Jul/2022:13:04:26 +0000] "GET / HTTP/1.1" 200 472 "-" "ELB-HealthChecker/2.0" "-"
192.0.6.198 - - [11/Jul/2022:13:04:26 +0000] "GET / HTTP/1.1" 200 472 "-" "ELB-HealthChecker/2.0" "-""#;
        let logs = parse_logs(logs);
        assert_eq!(logs["2022-07-11T13:04:26Z"].len(), 5);
        assert_eq!(
            logs["2022-07-11T13:04:26Z"][0].body,
            "/docker-entrypoint.sh: Launching /docker-entrypoint.d/20-envsubst-on-templates.sh"
        );
        assert_eq!(
            logs["2022-07-11T13:04:26Z"][1].body,
            "/docker-entrypoint.sh: Launching /docker-entrypoint.d/30-tune-worker-processes.sh"
        );
    }
}
