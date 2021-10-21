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
pub struct Notebook {
    #[serde(rename = "id")]
    pub id: String,
    #[serde(rename = "revision")]
    pub revision: i32,
    #[serde(rename = "title")]
    pub title: String,
    #[serde(rename = "cells")]
    pub cells: Vec<crate::models::Cell>,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "dataSources", skip_serializing_if = "Option::is_none")]
    pub data_sources: Option<::std::collections::HashMap<String, crate::models::NotebookDataSource>>,
    #[serde(rename = "timeRange")]
    pub time_range: Box<crate::models::TimeRange>,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
}

impl Notebook {
    pub fn new(id: String, revision: i32, title: String, cells: Vec<crate::models::Cell>, created_at: String, time_range: crate::models::TimeRange, updated_at: String) -> Notebook {
        Notebook {
            id,
            revision,
            title,
            cells,
            created_at,
            data_sources: None,
            time_range: Box::new(time_range),
            updated_at,
        }
    }
}


