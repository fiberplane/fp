use serde::{de::Error, Deserialize, Deserializer};
use time::format_description::FormatItem;
use time::{macros::format_description, OffsetDateTime};

const NGINX_TIMESTAMP_FORMAT: &[FormatItem] = format_description!("[day]/[month repr:short]/[year]:[hour repr:24]:[minute]:[second] [offset_hour sign:mandatory][offset_minute]");

/// This is a wrapper around `OffsetDateTime` that allows it to be deserialized a variety
/// of different timestamp formats.
#[derive(Deserialize, Debug, PartialEq)]
#[serde(untagged)]
pub enum AnyTimestamp {
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
    #[serde(deserialize_with = "deserialize_unix_timestamp_float")]
    UnixFloat(OffsetDateTime),
    #[serde(deserialize_with = "deserialize_unix_timestamp_float_string")]
    UnixFloatString(OffsetDateTime),
}

pub fn deserialize_nginx_timestamp<'de, D>(deserializer: D) -> Result<OffsetDateTime, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    OffsetDateTime::parse(&s, NGINX_TIMESTAMP_FORMAT).map_err(D::Error::custom)
}

pub fn deserialize_unix_timestamp_float<'de, D>(deserializer: D) -> Result<OffsetDateTime, D::Error>
where
    D: Deserializer<'de>,
{
    let f = f64::deserialize(deserializer)?;
    if f.is_nan() {
        Err(D::Error::custom("expected a valid Unix timestamp"))
    } else {
        let nanos = f * 1_000_000_000f64;
        OffsetDateTime::from_unix_timestamp_nanos(nanos as i128).map_err(D::Error::custom)
    }
}

pub fn deserialize_unix_timestamp_float_string<'de, D>(
    deserializer: D,
) -> Result<OffsetDateTime, D::Error>
where
    D: Deserializer<'de>,
{
    let f = String::deserialize(deserializer)?;
    let f: f64 = f.parse().map_err(D::Error::custom)?;
    if f.is_nan() {
        Err(D::Error::custom("expected a valid Unix timestamp"))
    } else {
        let nanos = f * 1_000_000_000f64;
        OffsetDateTime::from_unix_timestamp_nanos(nanos as i128).map_err(D::Error::custom)
    }
}

impl From<AnyTimestamp> for OffsetDateTime {
    fn from(timestamp: AnyTimestamp) -> Self {
        match timestamp {
            AnyTimestamp::Unix(t) => t,
            AnyTimestamp::Rfc3339(t) => t,
            AnyTimestamp::Iso8601(t) => t,
            AnyTimestamp::Rfc2822(t) => t,
            AnyTimestamp::Nginx(t) => t,
            AnyTimestamp::UnixFloat(t) => t,
            AnyTimestamp::UnixFloatString(t) => t,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use time::format_description::well_known::Rfc3339;

    #[test]
    fn unix() {
        let timestamp: AnyTimestamp = serde_json::from_value(json!(1657536964)).unwrap();
        assert_eq!(
            timestamp,
            AnyTimestamp::Unix(OffsetDateTime::parse("2022-07-11T10:56:04Z", &Rfc3339).unwrap())
        );
    }

    #[test]
    fn rfc3339() {
        let timestamp: AnyTimestamp =
            serde_json::from_value(json!("2022-07-11T10:56:04.2324317Z")).unwrap();
        assert_eq!(
            timestamp,
            AnyTimestamp::Rfc3339(
                OffsetDateTime::parse("2022-07-11T10:56:04.2324317Z", &Rfc3339).unwrap()
            )
        );
    }

    #[test]
    fn iso8601() {
        let timestamp: AnyTimestamp = serde_json::from_value(json!("20220711T105604Z")).unwrap();
        assert_eq!(
            timestamp,
            AnyTimestamp::Iso8601(OffsetDateTime::parse("2022-07-11T10:56:04Z", &Rfc3339).unwrap())
        );
    }

    #[test]
    fn rfc2822() {
        let timestamp: AnyTimestamp =
            serde_json::from_value(json!("Tue, 11 Jul 2022 10:56:04 GMT")).unwrap();
        assert_eq!(
            timestamp,
            AnyTimestamp::Rfc2822(OffsetDateTime::parse("2022-07-11T10:56:04Z", &Rfc3339).unwrap())
        );
    }

    #[test]
    fn nginx() {
        let timestamp: AnyTimestamp =
            serde_json::from_value(json!("11/Jul/2022:10:56:04 +0000")).unwrap();
        assert_eq!(
            timestamp,
            AnyTimestamp::Nginx(OffsetDateTime::parse("2022-07-11T10:56:04Z", &Rfc3339).unwrap())
        );
    }

    #[test]
    fn unix_float() {
        let timestamp: AnyTimestamp = serde_json::from_value(json!(1657536964.123456)).unwrap();
        assert_eq!(
            timestamp,
            AnyTimestamp::UnixFloat(
                OffsetDateTime::parse("2022-07-11T10:56:04.123456Z", &Rfc3339).unwrap()
            )
        );
    }

    #[test]
    fn unix_float_string() {
        let timestamp: AnyTimestamp = serde_json::from_value(json!("1657536964.123456")).unwrap();
        assert_eq!(
            timestamp,
            AnyTimestamp::UnixFloatString(
                OffsetDateTime::parse("2022-07-11T10:56:04.123456Z", &Rfc3339).unwrap()
            )
        );
    }
}
