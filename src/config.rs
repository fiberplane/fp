use anyhow::Error;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::io::ErrorKind;
use std::path::PathBuf;
use tokio::fs;
use toml;
use tracing::debug;

#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    #[serde(skip)]
    pub path: PathBuf,

    pub api_token: Option<String>,
}

impl Config {
    pub async fn load(path: Option<&str>) -> Result<Self, Error> {
        let path = parse_config_file_path(path)?;
        debug!("loading config from: {}", path.as_path().display());

        match fs::read_to_string(&path).await {
            Ok(string) => toml::from_str(&string).map_err(|e| e.into()),
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

fn parse_config_file_path(path: Option<&str>) -> Result<PathBuf, Error> {
    match path {
        Some(path) => {
            let path = PathBuf::from(path);
            if path.is_dir() {
                Ok(path.with_file_name("config.toml"))
            } else {
                Ok(path)
            }
        }
        None => Ok(default_config_file_path()),
    }
}

fn default_config_file_path() -> PathBuf {
    ProjectDirs::from("com", "Fiberplane", "fiberplane-cli")
        .unwrap()
        .config_dir()
        .to_owned()
        .join("config.toml")
}
