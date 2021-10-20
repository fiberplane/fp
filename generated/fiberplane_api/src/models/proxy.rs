/*
 * Fiberplane API
 *
 * No description provided (generated by Openapi Generator https://github.com/openapitools/openapi-generator)
 *
 * The version of the OpenAPI document: 1.0
 * 
 * Generated by: https://openapi-generator.tech
 */




#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Proxy {
    #[serde(rename = "id")]
    pub id: String,
    #[serde(rename = "status")]
    pub status: crate::models::ProxyConnectionStatus,
    #[serde(rename = "name")]
    pub name: String,
    /// this will only be set when creating a new proxy
    #[serde(rename = "token", skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    /// data-sources associated with this proxy
    #[serde(rename = "dataSources")]
    pub data_sources: Vec<crate::models::DataSourceSummary>,
}

impl Proxy {
    pub fn new(id: String, status: crate::models::ProxyConnectionStatus, name: String, data_sources: Vec<crate::models::DataSourceSummary>) -> Proxy {
        Proxy {
            id,
            status,
            name,
            token: None,
            data_sources,
        }
    }
}


