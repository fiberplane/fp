use anyhow::{anyhow, bail, Result};
use directories::ProjectDirs;
use fiberplane::api_client::clients::{default_config, ApiClient};
use hyper::http::HeaderValue;
use hyper::HeaderMap;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::io::ErrorKind;
use std::path::PathBuf;
use tokio::fs;
use tracing::{debug, info};
use url::Url;

use crate::MANIFEST;

pub static FP_CONFIG_DIR: Lazy<PathBuf> = Lazy::new(|| {
    ProjectDirs::from("com", "Fiberplane", "fiberplane-cli")
        .expect("home directory to exist")
        .config_dir()
        .to_owned()
});

#[derive(Serialize, Deserialize, Default, Debug)]
pub struct Config {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
}

impl Config {
    pub async fn load(profile_name: Option<&str>) -> Result<Self> {
        let profile_path = profile_or_default(profile_name).await?;

        let config_str = fs::read_to_string(&profile_path).await?;
        let config = toml::from_str(&config_str)?;

        Ok(config)
    }

    pub async fn save(&self, profile_name: Option<&str>) -> Result<()> {
        let profile_path = profile_or_default(profile_name).await?;

        fs::create_dir_all(
            profile_path
                .parent()
                .expect("fiberplane path should not resolve to root"),
        )
        .await?;

        let config = toml::to_string_pretty(&self)?;
        fs::write(&profile_path, config).await?;

        debug!("saved config to: {}", profile_path.as_path().display());
        Ok(())
    }
}

async fn profile_or_default(profile_name: Option<&str>) -> Result<PathBuf> {
    Ok(if let Some(profile_name) = profile_name {
        profile_path(profile_name)
    } else {
        default_profile_path().await?
    })
}

fn profile_path(profile_name: &str) -> PathBuf {
    FP_CONFIG_DIR.join(format!("{}.toml", profile_name.to_lowercase().trim_end()))
}

pub async fn default_profile_name() -> Result<String> {
    Ok(
        match fs::read_to_string(FP_CONFIG_DIR.join("default_profile")).await {
            Ok(default_profile) => default_profile,
            Err(err) => match err.kind() {
                ErrorKind::NotFound => "default".to_string(),
                _ => bail!("unable to read `default_profile` file: {err}"),
            },
        },
    )
}

async fn default_profile_path() -> Result<PathBuf> {
    let default_profile = default_profile_name().await?;
    Ok(FP_CONFIG_DIR.join(format!("{default_profile}.toml")))
}

pub(crate) async fn endpoint_url_for_endpoint(profile: Option<&str>) -> Result<String> {
    let config = Config::load(profile).await?;

    let endpoint = config
        .endpoint
        .unwrap_or_else(|| "https://studio.fiberplane.com".to_string());
    Ok(endpoint)
}

pub(crate) async fn api_client_configuration(profile: Option<&str>) -> Result<ApiClient> {
    let config = Config::load(profile).await?;

    let token = config.api_token.ok_or_else(|| {
        anyhow!("Must be logged in to run this command. Please run `fp login` first.")
    })?;

    let endpoint = config.endpoint.map_or_else(
        || Url::parse("https://studio.fiberplane.com"),
        |input| Url::parse(&input),
    )?;

    api_client_configuration_from_token(&token, endpoint)
}

pub(crate) fn api_client_configuration_from_token(token: &str, base_url: Url) -> Result<ApiClient> {
    let mut headers = HeaderMap::new();
    headers.insert(
        "Authorization",
        HeaderValue::from_str(&format!("Bearer {token}"))?,
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

#[derive(Deserialize, Debug)]
struct OldConfig {
    pub api_token: Option<String>,
}

impl From<OldConfig> for Config {
    fn from(value: OldConfig) -> Self {
        Config {
            api_token: value.api_token,
            ..Default::default()
        }
    }
}

pub async fn init() -> Result<()> {
    // migrate old config format first if it exists
    migrate().await?;

    let default_path = default_profile_path().await?;

    if let Err(err) = fs::metadata(default_path).await {
        debug!("failed to read default profile file, creating it: {err}");
        Config::default().save(None).await?;
    }

    Ok(())
}

/// Migrates from the old config format to the new one
async fn migrate() -> Result<()> {
    let old_config_path = FP_CONFIG_DIR.join("config.toml");

    if fs::metadata(&old_config_path).await.is_err() {
        return Ok(());
    }

    info!("Detected old config format, migrating to new format...");

    let config_str = fs::read_to_string(&old_config_path).await?;

    let old_config: OldConfig = toml::from_str(&config_str)?;
    let new_config: Config = old_config.into();

    new_config.save(None).await?;
    fs::remove_file(old_config_path).await?;

    info!("Successfully migrated to the new config format");
    Ok(())
}
