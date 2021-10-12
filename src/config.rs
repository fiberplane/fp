use anyhow::Error;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::io::ErrorKind;
use std::{path::PathBuf, str::FromStr};
use tokio::fs;
use toml;

#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    #[serde(skip)]
    pub path: PathBuf,

    pub api_token: Option<String>,
}

impl Config {
    pub async fn load(path: Option<&str>) -> Result<Self, Error> {
        let path = parse_config_file_path(path)?;
        match fs::read_to_string(&path).await {
            Ok(string) => toml::from_str(&string).map_err(|e| e.into()),
            // TODO should we create an empty file here if one does not already exist?
            Err(err) if err.kind() == ErrorKind::NotFound => Ok(Config {
                path,
                api_token: None,
            }),
            Err(err) => Err(err.into()),
        }
    }

    pub async fn save(&self) -> Result<(), Error> {
        let string = toml::to_string_pretty(&self)?;
        fs::write(&self.path, string).await?;
        Ok(())
    }
}

fn parse_config_file_path(path: Option<&str>) -> Result<PathBuf, Error> {
    match path {
        Some(path) => PathBuf::from_str(path).map_err(|e| e.into()),
        None => Ok(default_config_file_path()),
    }
}

fn default_config_file_path() -> PathBuf {
    ProjectDirs::from("com", "Fiberplane", "fiberplane-cli")
        .unwrap()
        .config_dir()
        .to_owned()
}
