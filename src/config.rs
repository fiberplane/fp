use anyhow::{anyhow, Error, Result};
use directories::ProjectDirs;
use fiberplane::api_client::clients::{default_config, ApiClient};
use hyper::http::HeaderValue;
use hyper::HeaderMap;
use serde::{Deserialize, Serialize};
use std::io::ErrorKind;
use std::path::PathBuf;
use tokio::fs;
use tracing::debug;
use url::Url;

use crate::MANIFEST;

#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    #[serde(skip)]
    pub path: PathBuf,

    pub api_token: Option<String>,
}

impl Config {
    pub async fn load(path: Option<PathBuf>) -> Result<Self, Error> {
        let path = path_or_default(path);
        debug!("loading config from: {}", path.as_path().display());

        match fs::read_to_string(&path).await {
            Ok(string) => {
                let mut config: Config = toml::from_str(&string).map_err(Error::from)?;
                config.path = path;
                Ok(config)
            }
            // TODO should we create an empty file here if one does not already exist?
            Err(err) if err.kind() == ErrorKind::NotFound => {
                debug!("no config file found, using default config");
                Ok(Config {
                    path,
                    api_token: None,
                })
            }
            Err(err) => Err(err.into()),
        }
    }

    pub async fn save(&self) -> Result<(), Error> {
        let string = toml::to_string_pretty(&self)?;
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir).await?;
        }
        fs::write(&self.path, string).await?;
        debug!("saved config to: {}", self.path.as_path().display());
        Ok(())
    }
}

/// Returns the path if it is set and does not look like a directory, if it does
/// look like a directory, then append config.toml to it. Finally if nothing is
/// set then use the default path.
fn path_or_default(path: Option<PathBuf>) -> PathBuf {
    match path {
        Some(path) => {
            if path.is_dir() {
                path.with_file_name("config.toml")
            } else {
                path
            }
        }
        None => default_config_file_path(),
    }
}

fn default_config_file_path() -> PathBuf {
    ProjectDirs::from("com", "Fiberplane", "fiberplane-cli")
        .unwrap()
        .config_dir()
        .to_owned()
        .join("config.toml")
}

pub(crate) async fn api_client_configuration(
    config_path: Option<PathBuf>,
    base_url: Url,
) -> Result<ApiClient> {
    let token = Config::load(config_path).await?.api_token.ok_or_else(|| {
        anyhow!("Must be logged in to run this command. Please run `fp login` first.")
    })?;

    api_client_configuration_from_token(&token, base_url)
}

pub(crate) fn api_client_configuration_from_token(token: &str, base_url: Url) -> Result<ApiClient> {
    let mut headers = HeaderMap::new();
    headers.insert(
        "Authorization",
        HeaderValue::from_str(&format!("Bearer {}", token))?,
    );

    let client = default_config(
        None,
        Some(&format!("fp {}", MANIFEST.build_version)),
        Some(headers),
    )?;

    Ok(ApiClient {
        client,
        server: base_url,
    })
}
