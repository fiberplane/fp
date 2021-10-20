/*
 * Fiberplane API
 *
 * No description provided (generated by Openapi Generator https://github.com/openapitools/openapi-generator)
 *
 * The version of the OpenAPI document: 1.0
 * 
 * Generated by: https://openapi-generator.tech
 */


/// 
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub enum PointType {
    #[serde(rename = "f64")]
    F64,
    #[serde(rename = "string")]
    String,

}

impl ToString for PointType {
    fn to_string(&self) -> String {
        match self {
            Self::F64 => String::from("f64"),
            Self::String => String::from("string"),
        }
    }
}




