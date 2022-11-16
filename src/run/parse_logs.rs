use super::timestamp::AnyTimestamp;
use fiberplane::protocols::providers::{Event, OtelMetadata, OtelSpanId, OtelTraceId};
use grok::{Grok, Pattern};
use once_cell::sync::Lazy;
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::convert::TryInto;
use time::OffsetDateTime;
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
pub fn parse_logs(output: &str) -> Vec<Event> {
    let mut logs = Vec::new();
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
            Some(record) => {
                // If we had lines before that didn't have timestamps, add them
                // under this timestamp:
                if !lines_without_timestamps.is_empty() {
                    logs.extend(lines_without_timestamps.drain(..).map(|line| Event {
                        time: record.time,
                        title: line,
                        description: None,
                        end_time: None,
                        labels: BTreeMap::new(),
                        otel: OtelMetadata::default(),
                        severity: None,
                    }));
                }

                most_recent_timestamp = Some(record.time);
                logs.push(record);
            }
            None => {
                if let Some(timestamp) = &most_recent_timestamp {
                    logs.push(Event {
                        time: *timestamp,
                        title: line.to_string(),
                        description: None,
                        end_time: None,
                        labels: BTreeMap::new(),
                        otel: OtelMetadata::default(),
                        severity: None,
                    })
                } else {
                    lines_without_timestamps.push(line.to_string());
                }
            }
        }
    }

    // If none of the lines had timestamps, use the current moment as the timestamp
    if !lines_without_timestamps.is_empty() {
        let now = OffsetDateTime::now_utc();
        logs.extend(lines_without_timestamps.drain(..).map(|line| Event {
            time: now.into(),
            title: line,
            description: None,
            end_time: None,
            labels: BTreeMap::new(),
            otel: OtelMetadata::default(),
            severity: None,
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

fn parse_log(line: &str) -> Option<Event> {
    if let Ok(Value::Object(json)) = serde_json::from_str(line) {
        parse_json(json)
    } else if let Some(matches) = NGINX_PATTERN
        .match_against(line)
        .or_else(|| GITHUB_ACTION_PATTERN.match_against(line))
    {
        let fields = matches
            .into_iter()
            // The keys written in upper case are the grok components used to
            // build up the values we care about
            .filter(|(k, _)| k.chars().all(|c| c.is_lowercase()))
            .map(|(k, v)| {
                (
                    k.to_string(),
                    Value::String(v.trim_matches('"').to_string()),
                )
            })
            .collect();
        parse_flattened_json(fields)
    } else {
        None
    }
}

fn parse_json(json: Map<String, Value>) -> Option<Event> {
    let mut flattened_fields = BTreeMap::new();
    for (key, val) in json.into_iter() {
        flatten_nested_value(&mut flattened_fields, key, val);
    }
    parse_flattened_json(flattened_fields)
}

fn parse_flattened_json(mut json: BTreeMap<String, Value>) -> Option<Event> {
    let trace_id = json.remove("trace_id").or_else(|| json.remove("trace.id"));
    let span_id = json.remove("span_id").or_else(|| json.remove("span.id"));

    // Find the timestamp field (or set it to NaN if none is found)
    // Note: this will leave the original timestamp field in the flattened_fields
    let mut timestamp: Option<OffsetDateTime> = None;
    for field_name in TIMESTAMP_FIELDS {
        if let Some(ts) = json.remove(*field_name) {
            match serde_json::from_value::<AnyTimestamp>(ts) {
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

    // Find the body field (or set it to an empty string if none is found)
    // Note: this will leave the body field in the flattened_fields and copy
    // it into the body of the LogRecord
    let mut body = String::new();
    for field_name in BODY_FIELDS {
        if let Some(b) = json.remove(*field_name) {
            body = match b.as_str() {
                Some(str) => str.to_owned(),
                None => b.to_string(),
            };
            break;
        }
    }

    // All fields that are not mapped to the resource field
    // become part of the attributes field
    // TODO refactor this so we only make one pass over the fields
    let (resource, attributes): (BTreeMap<String, Value>, BTreeMap<String, Value>) =
        json.into_iter().partition(|(key, _)| {
            RESOURCE_FIELD_PREFIXES
                .iter()
                .any(|prefix| key.starts_with(prefix))
                && !RESOURCE_FIELD_EXCEPTIONS.contains(&key.as_str())
        });

    timestamp.map(|timestamp| Event {
        time: timestamp.into(),
        title: body,
        description: None,
        end_time: None,
        labels: BTreeMap::new(),
        otel: OtelMetadata {
            attributes,
            resource,
            span_id: span_id.and_then(|span| {
                span.as_str()
                    .and_then(|span| span.as_bytes().try_into().ok().map(OtelSpanId))
            }),
            trace_id: trace_id.and_then(|trace| {
                trace
                    .as_str()
                    .and_then(|trace| trace.as_bytes().try_into().ok().map(OtelTraceId))
            }),
        },
        severity: None,
    })
}

fn flatten_nested_value(output: &mut BTreeMap<String, Value>, key: String, value: Value) {
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
        v => {
            output.insert(key, v);
        }
    };
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use time::format_description::well_known::Rfc3339;

    use super::*;
    use std::iter::FromIterator;

    #[test]
    fn json_logs() {
        let logs = r#"{"ts": "2018-01-01T00:00:00.000Z", "body": "test"}
        {"timestamp": "1657619253", "message": "hello", "trace_id": "1234567890123456", "thing": 1, "host.name": "blah"}"#;
        let logs = parse_logs(logs);
        assert_eq!(logs.len(), 2);

        assert_eq!(logs[0].title, "test");
        assert_eq!(
            logs[0].time.0,
            OffsetDateTime::parse("2018-01-01T00:00:00Z", &Rfc3339).unwrap()
        );

        assert_eq!(logs[1].title, "hello");
        assert_eq!(
            logs[1].time.0,
            OffsetDateTime::parse("2022-07-12T09:47:33Z", &Rfc3339).unwrap()
        );

        assert_eq!(
            logs[1].otel.trace_id,
            Some(OtelTraceId(
                "1234567890123456".as_bytes().try_into().unwrap()
            ))
        );
        assert_eq!(logs[1].otel.attributes["thing"], json!(1));
        assert_eq!(logs[1].otel.resource["host.name"], "blah");
    }

    #[test]
    fn nginx_logs() {
        let logs = r#"
192.0.7.128 - - [11/Jul/2022:13:04:26 +0000] "GET / HTTP/1.1" 200 472 "-" "ELB-HealthChecker/2.0" "-"
192.0.6.198 - - [11/Jul/2022:13:04:27 +0000] "GET / HTTP/1.1" 200 472 "-" "ELB-HealthChecker/2.0" "-""#;
        let logs = parse_logs(logs);
        assert_eq!(logs.len(), 2);

        let mut attributes = BTreeMap::from_iter(
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
            .map(|(k, v)| (k.to_string(), Value::String(v.to_string()))),
        );
        assert_eq!(logs[0].otel.attributes, attributes);

        *attributes.get_mut("clientip").unwrap() = Value::String("192.0.6.198".to_string());
        assert_eq!(logs[1].otel.attributes, attributes);
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
            logs[0].time.0,
            OffsetDateTime::parse("2022-07-11T15:12:28.2324317Z", &Rfc3339).unwrap()
        );
        assert_eq!(logs[0].title, "Packages: write");
        assert_eq!(logs[0].otel.attributes["job"], "build");
        assert_eq!(logs[0].otel.attributes["step"], "Set up job");
    }

    #[test]
    fn no_timestamps() {
        let logs = r#"
/docker-entrypoint.sh: Launching /docker-entrypoint.d/20-envsubst-on-templates.sh
/docker-entrypoint.sh: Launching /docker-entrypoint.d/30-tune-worker-processes.sh
/docker-entrypoint.sh: Configuration complete; ready for start up"#;
        let logs = parse_logs(logs);
        assert_eq!(logs.len(), 3);
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
        assert_eq!(logs.len(), 5);

        assert_eq!(
            logs[0].time.0,
            OffsetDateTime::parse("2022-07-11T13:04:26Z", &Rfc3339).unwrap()
        );
        assert_eq!(
            logs[0].title,
            "/docker-entrypoint.sh: Launching /docker-entrypoint.d/20-envsubst-on-templates.sh"
        );
        assert_eq!(
            logs[1].title,
            "/docker-entrypoint.sh: Launching /docker-entrypoint.d/30-tune-worker-processes.sh"
        );
    }
}
