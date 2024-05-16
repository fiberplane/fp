use crate::MANIFEST;
use anyhow::{anyhow, bail, Context, Error, Result};
use directories::ProjectDirs;
use fiberplane::api_client::clients::default_config;
use fiberplane::api_client::ApiClient;
use http::{HeaderMap, HeaderValue};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::fs;
use tracing::debug;
use url::Url;

pub static FP_PROFILES_DIR: Lazy<PathBuf> = Lazy::new(|| {
    ProjectDirs::from("com", "Fiberplane", "fiberplane-cli")
        .expect("home directory to exist")
        .config_dir()
        .join("profiles")
        .to_owned()
});

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct Config {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
}

impl Config {
    /// loads `profile` as a `Config` from the corresponding file. if passed
    /// argument is `None`, default profile will be used
    pub async fn load(profile: Option<String>) -> Result<Self, Error> {
        let path = if let Some(profile) = profile {
            profile_path(&profile)
        } else {
            let path = FP_PROFILES_DIR.join("default");

            if !path.is_file() {
                Config::default().save("default").await
                    .context("failed to save default profile")?;

                make_default("default").await
                    .context("failed to make profile `default` into default profile")?;
            }

            path
        };

        let content = fs::read_to_string(path)
            .await
            .with_context(|| format!("failed to load profile `{profile}`"))?;

        let config: Config =
            toml::from_str(&content).context("failed to parse profile file as toml")?;

        Ok(config)
    }

    pub async fn save(&self, profile: &str) -> Result<()> {
        let path = profile_path(profile);

        fs::create_dir_all(path.parent().expect("profiles dir not to be at root")).await
            .context("failed to create profiles directory")?;

        let config = toml::to_string_pretty(&self)
            .context("failed to serialize config struct into toml")?;

        fs::write(path, config).await
            .context("failed to write config to disk")?;

        debug!("saved profile to: {}", path.as_path().display());
        Ok(())
    }
}

/// returns path of profile. does not actually check if it exists
fn profile_path(profile: &str) -> PathBuf {
    FP_PROFILES_DIR.join(if !profile.ends_with(".toml") {
        let mut profile = profile.to_string();
        profile.push_str(".toml");

        profile
    } else {
        profile
    })
}

pub(crate) async fn is_default(profile: &str) -> Result<bool> {
    let path = profile_path(profile);

    if !path.is_file() {
        bail!("profile named {} not found", path.display());
    }

    let resolved_path = tokio::fs::read_link(FP_PROFILES_DIR.join("default")).await
        .context("failed to resolve symlink to default profile")?;

    Ok(path == resolved_path)
}

pub(crate) async fn make_default(profile: &str) -> Result<()> {
    let path = profile_path(profile);

    if !path.is_file() {
        bail!("cannot make non existent file the default profile");
    }

    let default_path = FP_PROFILES_DIR.join("default");

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    tokio::fs::symlink(path, default_path).await
        .with_context(|| format!("failed to symlink {profile} into the default profile"))?;

    #[cfg(target_os = "windows")]
    std::os::windows::fs::symlink_file(path, default_path)
        .with_context(|| format!("failed to symlink {profile} into the default profile"))?;

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    bail!("symlinks are not supported on your platform");

    Ok(())
}

/// Returns a ApiClient with the token set. If `token` is `Some` then that will
/// be used, otherwise the config will be used for the token.
pub(crate) async fn api_client_configuration(
    token: Option<String>,
    profile: Option<String>,
    base_url: Option<Url>,
) -> Result<ApiClient> {
    let config = Config::load(profile).await?;

    let token = if let Some(token) = token {
        token
    } else {
        config.api_token.ok_or_else(|| anyhow!("Must be logged in to run this command. Please run `fp login` first."))?
    };

    let base_url = if let Some(base_url) = base_url {
        base_url
    } else {
        Url::parse(config.endpoint.as_deref().unwrap_or("https://studio.fiberplane.com"))
            .context("failed to parse endpoint in config file as `Url`")?
    };

    api_client_configuration_from_token(&token, base_url)
}

pub fn api_client_configuration_from_token(token: &str, base_url: Url) -> Result<ApiClient> {
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
