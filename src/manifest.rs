use crate::output::GenericKeyValue;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone)]
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
            build_version: env!("CARGO_PKG_VERSION").to_owned(),
            commit_date: env!("VERGEN_GIT_COMMIT_TIMESTAMP").to_owned(),
            commit_sha: env!("VERGEN_GIT_SHA").to_owned(),
            commit_branch: env!("VERGEN_GIT_BRANCH").to_owned(),
            rustc_version: env!("VERGEN_RUSTC_SEMVER").to_owned(),
            rustc_channel: env!("VERGEN_RUSTC_CHANNEL").to_owned(),
            rustc_host_triple: env!("VERGEN_RUSTC_HOST_TRIPLE").to_owned(),
            rustc_commit_sha: env!("VERGEN_RUSTC_COMMIT_HASH").to_owned(),
            cargo_target_triple: env!("VERGEN_CARGO_TARGET_TRIPLE").to_owned(),
            cargo_profile: env!("VERGEN_CARGO_OPT_LEVEL").to_owned(),
        }
    }
}

impl GenericKeyValue {
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
            GenericKeyValue::new("cargo Opt Level:", manifest.cargo_profile),
        ]
    }
}
