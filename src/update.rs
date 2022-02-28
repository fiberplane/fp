use crate::MANIFEST;
use anyhow::{anyhow, Result};
use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use sha2::{Digest, Sha256};
use std::io::{BufWriter, Write};
use std::os::unix::prelude::OpenOptionsExt;
use tracing::{debug, info};

#[derive(Parser)]
pub struct Arguments {}

pub async fn handle_command(_args: Arguments) -> Result<()> {
    // First check if the latest version is not the same as the current version
    let latest_version = retrieve_latest_version().await?;
    if latest_version == *MANIFEST.build_version {
        info!("Already running the latest version.");
        return Ok(());
    } else {
        info!("Updating to version {}", latest_version);
    };

    // Create a temporary file to buffer the download.
    let temp_file_path = std::env::temp_dir().join("fp-tmp");
    let temp_file = std::fs::OpenOptions::new()
        .mode(0o755) // This will only work on Unix-like operating systems at the moment
        .write(true)
        .create(true)
        .truncate(true)
        .open(&temp_file_path)?;
    let mut temp_file = BufWriter::new(temp_file);

    // Fetch latest binary for current host-triple to the temporary file.
    let arch = &*crate::MANIFEST.cargo_target_triple;

    let mut res = reqwest::get(format!("https://fp.dev/fp/v{latest_version}/{arch}/fp"))
        .await?
        .error_for_status()?;
    let total_size = res.content_length().unwrap();

    let pb = ProgressBar::new(total_size);
    pb.set_style(ProgressStyle::default_bar()
        .template("{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}, {eta})")
        .progress_chars("#>-"));

    let mut sha256_hasher = Sha256::new();

    // Write the chunks to the buffered writer, while updating the progress-bar.
    let mut downloaded = 0;
    while let Some(chunk) = res.chunk().await? {
        temp_file.write_all(&chunk)?;
        sha256_hasher.write_all(&chunk)?;

        downloaded += chunk.len();
        pb.set_position(downloaded as u64);
    }

    // Calculate the final sha256 sum and compare it to the remote sha256 sum
    let remote_sha256_sum = retrieve_sha256_sum(&latest_version, arch).await?;
    let computed_sha256_sum = base16ct::lower::encode_string(&sha256_hasher.finalize());

    if remote_sha256_sum != computed_sha256_sum {
        debug!(
            %remote_sha256_sum,
            %computed_sha256_sum, "Remote sha256 sum does not match the calculated sha256 sum"
        );
        return Err(anyhow!(
            "Remote sha256 sum does not match the calculated sha256 sum"
        ));
    }

    // Make sure that everything is written to disk and that we closed the file.
    temp_file.flush()?;
    drop(temp_file);

    pb.finish_with_message("downloaded");

    // Override the current executable.
    let current_exe = std::env::current_exe()?;
    std::fs::rename(temp_file_path, current_exe)?;

    info!("Updated to version {}", latest_version);

    Ok(())
}

/// Retrieve the latest version available.
pub async fn retrieve_latest_version() -> Result<String> {
    let version_url = "https://fp.dev/fp/latest/version";
    let latest_version = reqwest::get(version_url)
        .await?
        .error_for_status()?
        .text()
        .await?;

    Ok(latest_version.trim().to_owned())
}

/// Retrieve the sha256 digest for the fp binary for the specified version and
/// architecture. If `fp` is not found within the checksums.sha256 file it will
/// return an error.
pub async fn retrieve_sha256_sum(version: &str, arch: &str) -> Result<String> {
    let response = reqwest::get(format!(
        "https://fp.dev/fp/v{version}/{arch}/checksum.sha256"
    ))
    .await?
    .error_for_status()?
    .text()
    .await?;

    // Search through the lines for a file name `fp`, if not found a error will
    // be returned.
    response
        .lines()
        .find_map(|line| match line.split_once("  ") {
            Some((sha256_sum, file)) if file == "fp" => Some(sha256_sum.to_owned()),
            _ => None,
        })
        .map_or_else(
            || Err(anyhow!("version not found in checksum.sha256")),
            |sha256_sum| Ok(sha256_sum),
        )
}
