use serde::{de::Error, Deserialize, Deserializer};
use time::format_description::FormatItem;
use time::{macros::format_description, OffsetDateTime};

const NGINX_TIMESTAMP_FORMAT: &[FormatItem] = format_description!("[day]/[month repr:short]/[year]:[hour repr:24]:[minute]:[second] [offset_hour sign:mandatory][offset_minute]");

#[derive(Deserialize, Debug)]
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