use anyhow::{Context, Result};
use reqwest;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct Manifest {
    pub build_timestamp: String,
    pub build_version: String,
    pub commit_date: String,
    pub commit_sha: String,
    pub commit_branch: String,
    pub rustc_version: String,
    pub rustc_channel: String,
    pub rustc_host_triple: String,
    pub rustc_commit_sha: String,
    pub cargo_target_triple: String,
    pub cargo_profile: String,
}

impl Manifest {
    pub fn from_env() -> Manifest {
        Manifest {
            build_timestamp: env!("VERGEN_BUILD_TIMESTAMP").to_owned(),
            build_version: env!("VERGEN_GIT_SEMVER").to_owned(),
            commit_date: env!("VERGEN_GIT_COMMIT_TIMESTAMP").to_owned(),
            commit_sha: env!("VERGEN_GIT_SHA").to_owned(),
            commit_branch: env!("VERGEN_GIT_BRANCH").to_owned(),
            rustc_version: env!("VERGEN_RUSTC_SEMVER").to_owned(),
            rustc_channel: env!("VERGEN_RUSTC_CHANNEL").to_owned(),
            rustc_host_triple: env!("VERGEN_RUSTC_HOST_TRIPLE").to_owned(),
            rustc_commit_sha: env!("VERGEN_RUSTC_COMMIT_HASH").to_owned(),
            cargo_target_triple: env!("VERGEN_CARGO_TARGET_TRIPLE").to_owned(),
            cargo_profile: env!("VERGEN_CARGO_PROFILE").to_owned(),
        }
    }
}

/// Retrieve the latest manifest of a fp binary for the specified host triple
pub async fn retrieve_manifest(host_triple: &str) -> Result<Manifest> {
    let manifest_url = format!("https://fp.dev/fp/latest/{host_triple}/manifest.json");
    let manifest = reqwest::get(manifest_url)
        .await?
        .error_for_status()?
        .bytes()
        .await?;

    let manifest =
        serde_json::from_slice(&manifest).context("failed to serialize the version manifest")?;

    Ok(manifest)
}
