use anyhow::Result;
use cli_table::format::*;
use cli_table::{print_stdout, Row, Table, Title};
use fp_api_client::models::*;
use reqwest::Url;

use crate::manifest::Manifest;

pub fn output_list<T, R>(input: T) -> Result<()>
where
    T: IntoIterator<Item = R>,
    R: Row + Title,
{
    print_stdout(
        input
            .table()
            .title(R::title())
            .border(Border::builder().build())
            .separator(Separator::builder().build()),
    )
    .map_err(Into::into)
}

pub fn output_details<T, R>(args: T) -> Result<()>
where
    T: IntoIterator<Item = R>,
    R: Row,
{
    print_stdout(
        args.table()
            .border(Border::builder().build())
            .separator(Separator::builder().build()),
    )
    .map_err(Into::into)
}

#[derive(Table)]
pub struct GenericKeyValue {
    #[table(title = "key", justify = "Justify::Right")]
    key: String,

    #[table(title = "value")]
    value: String,
}

impl GenericKeyValue {
    #[must_use]
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
        }
    }

    pub fn from_proxy(proxy: Proxy) -> Vec<GenericKeyValue> {
        let datasources = if proxy.data_sources.is_empty() {
            String::from("(none)")
        } else {
            let mut datasources = String::new();
            for datasource in proxy.data_sources {
                datasources.push_str(&format!("{} ({:?})\n", datasource.name, datasource._type))
            }
            datasources
        };

        vec![
            GenericKeyValue::new("Name:", proxy.name),
            GenericKeyValue::new("ID:", proxy.id),
            GenericKeyValue::new("Status:", proxy.status.to_string()),
            GenericKeyValue::new("Datasources:", datasources),
        ]
    }

    pub fn from_template(template: Template) -> Vec<GenericKeyValue> {
        vec![
            GenericKeyValue::new("Title:", template.title),
            GenericKeyValue::new("ID:", template.id),
            GenericKeyValue::new(
                "Parameters:",
                format_template_parameters(template.parameters),
            ),
            GenericKeyValue::new("Body:", template.body),
        ]
    }

    pub fn from_trigger(trigger: Trigger, base_url: Url) -> Vec<GenericKeyValue> {
        let invoke_url = format!(
            "{}api/triggers/{}/{}",
            base_url,
            trigger.id,
            trigger
                .secret_key
                .unwrap_or_else(|| String::from("<secret_key>"))
        );

        vec![
            GenericKeyValue::new("Title:", trigger.title),
            GenericKeyValue::new("ID:", trigger.id),
            GenericKeyValue::new("Invoke URL:", invoke_url),
            GenericKeyValue::new("Template ID:", trigger.template_id),
        ]
    }

    pub fn from_manifest(manifest: Manifest) -> Vec<GenericKeyValue> {
        vec![
            GenericKeyValue::new("Build Timestamp:", manifest.build_timestamp),
            GenericKeyValue::new("Build Version:", manifest.build_version),
            GenericKeyValue::new("Commit Date:", manifest.commit_date),
            GenericKeyValue::new("Commit SHA:", manifest.commit_sha),
            GenericKeyValue::new("Commit Branch:", manifest.commit_branch),
            GenericKeyValue::new("rustc Version:", manifest.rustc_version),
            GenericKeyValue::new("rustc Channel:", manifest.rustc_channel),
            GenericKeyValue::new("rustc Host Triple:", manifest.rustc_host_triple),
            GenericKeyValue::new("rustc Commit SHA:", manifest.rustc_commit_sha),
            GenericKeyValue::new("cargo Target Triple:", manifest.cargo_target_triple),
            GenericKeyValue::new("cargo Profile:", manifest.cargo_profile),
        ]
    }
}

#[derive(Table)]
pub struct ProxySummaryRow {
    #[table(title = "ID")]
    pub id: String,

    #[table(title = "Name")]
    pub name: String,

    #[table(title = "Status")]
    pub status: String,
}

impl From<ProxySummary> for ProxySummaryRow {
    fn from(proxy: ProxySummary) -> Self {
        Self {
            id: proxy.id,
            name: proxy.name,
            status: proxy.status.to_string(),
        }
    }
}

#[derive(Table)]
pub struct DataSourceAndProxySummaryRow {
    #[table(title = "Name")]
    pub name: String,

    #[table(title = "Type")]
    pub _type: String,

    #[table(title = "Status")]
    pub status: String,

    #[table(title = "Proxy name")]
    pub proxy_name: String,

    #[table(title = "Proxy ID")]
    pub proxy_id: String,

    #[table(title = "Proxy status")]
    pub proxy_status: String,
}

impl From<DataSourceAndProxySummary> for DataSourceAndProxySummaryRow {
    fn from(data_source_and_proxy_summary: DataSourceAndProxySummary) -> Self {
        Self {
            name: data_source_and_proxy_summary.name,
            _type: data_source_and_proxy_summary._type.to_string(),
            status: data_source_and_proxy_summary
                .error_message
                .unwrap_or_else(|| "connected".to_string()),
            proxy_name: data_source_and_proxy_summary.proxy.name,
            proxy_id: data_source_and_proxy_summary.proxy.id,
            proxy_status: data_source_and_proxy_summary.proxy.status.to_string(),
        }
    }
}

#[derive(Table)]
pub struct TemplateRow {
    #[table(title = "Title")]
    pub title: String,

    #[table(title = "ID")]
    pub id: String,

    #[table(title = "Updated at")]
    pub updated_at: String,

    #[table(title = "Created at")]
    pub created_at: String,
}

impl From<TemplateSummary> for TemplateRow {
    fn from(template: TemplateSummary) -> Self {
        Self {
            id: template.id,
            title: template.title,
            updated_at: template.updated_at,
            created_at: template.created_at,
        }
    }
}

impl From<Template> for TemplateRow {
    fn from(template: Template) -> Self {
        Self {
            id: template.id,
            title: template.title,
            updated_at: template.updated_at,
            created_at: template.created_at,
        }
    }
}

fn format_template_parameters(parameters: Vec<TemplateParameter>) -> String {
    if parameters.is_empty() {
        return String::from("(none)");
    }

    let mut result: Vec<String> = vec![];
    for parameter in parameters {
        match parameter {
            TemplateParameter::StringTemplateParameter {
                name,
                default_value,
            } => {
                result.push(format!(
                    "{}: string (default: \"{}\")",
                    name,
                    default_value.unwrap_or_default()
                ));
            }
            TemplateParameter::NumberTemplateParameter {
                name,
                default_value,
            } => {
                result.push(format!(
                    "{}: number (default: {})",
                    name,
                    default_value.unwrap_or_default()
                ));
            }
            TemplateParameter::BooleanTemplateParameter {
                name,
                default_value,
            } => {
                result.push(format!(
                    "{}: boolean (default: {})",
                    name,
                    default_value.unwrap_or_default()
                ));
            }
            TemplateParameter::ArrayTemplateParameter {
                name,
                default_value,
            } => {
                result.push(format!(
                    "{}: array (default: {})",
                    name,
                    serde_json::to_string(&default_value).unwrap()
                ));
            }
            TemplateParameter::ObjectTemplateParameter {
                name,
                default_value,
            } => {
                result.push(format!(
                    "{}: object (default: {})",
                    name,
                    serde_json::to_string(&default_value).unwrap()
                ));
            }
            TemplateParameter::UnknownTemplateParameter { name } => {
                result.push(format!("{}: (type unknown)", name));
            }
        };
    }

    result.join("\n")
}
