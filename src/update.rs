use crate::MANIFEST;
use anyhow::{anyhow, Result};
use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use sha2::{Digest, Sha256};
use std::env;
use std::fs::OpenOptions;
use std::io::{BufWriter, Write};
use tracing::{debug, info};

#[cfg(not(windows))]
use std::os::unix::prelude::OpenOptionsExt;

#[derive(Parser)]
pub struct Arguments {
    #[clap(long, short)]
    force: bool,
}

pub async fn handle_command(args: Arguments) -> Result<()> {
    // First check if the latest version is not the same as the current version
    let latest_version = retrieve_latest_version().await?;
    if latest_version == *MANIFEST.build_version {
        info!("Already running the latest version.");
        return Ok(());
    } else if args.force {
        info!("Forcing update to version {}", latest_version);
    } else if installed_through_homebrew() {
        info!("A new version of fp is available: {}", latest_version);
        info!("You can update fp by running `brew upgrade fp` (or use `fp update --force`)");
    } else {
        info!("Updating to version {}", latest_version);
    };

    let current_exe = std::env::current_exe()?;

    // Create a temporary file to buffer the download.
    let temp_file_path = current_exe.parent().unwrap().join("fp_update");
    let temp_file = OpenOptions::new()
        .mode(0o755) // This call is a no-op on Windows
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
        .expect("the template should never be invalid")
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

    // Calculate the final sha256 hash and compare it to the remote sha256 hash
    let remote_sha256_hash = retrieve_sha256_hash(&latest_version, arch).await?;
    let computed_sha256_hash = hex::encode(sha256_hasher.finalize());

    if remote_sha256_hash != computed_sha256_hash {
        debug!(
            %remote_sha256_hash,
            %computed_sha256_hash, "Calculated sha256 hash does not match the remote sha256 hash"
        );
        return Err(anyhow!(
            "Calculated sha256 hash does not match the remote sha256 hash"
        ));
    }

    // Make sure that everything is written to disk and that we closed the file.
    temp_file.flush()?;
    drop(temp_file);

    pb.finish_with_message("downloaded");

    // Override the current executable.
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

/// Retrieve the sha256 hash for the fp binary for the specified version and
/// architecture. If `fp` is not found within the checksums.sha256 file it will
/// return an error.
pub async fn retrieve_sha256_hash(version: &str, arch: &str) -> Result<String> {
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
            Some((sha256_hash, file)) if file == "fp" => Some(sha256_hash.to_owned()),
            _ => None,
        })
        .map_or_else(|| Err(anyhow!("version not found in checksum.sha256")), Ok)
}

#[cfg(windows)]
trait DummyOpenOptionsExt {
    /// Mirror of [unix fs' OpenOptionsExt::mode](std::os::unix::fs::OpenOptionsExt::mode).
    /// This function is a no-op.
    fn mode(&mut self, _: u32) -> &mut Self;
}

#[cfg(windows)]
impl DummyOpenOptionsExt for OpenOptions {
    fn mode(&mut self, _: u32) -> &mut Self {
        self
    }
}

/// A naive way of checking if fp is installed through homebrew.
///
/// This will check if the current executable is located in the default linux
/// homebrew location: `/home/linuxbrew/.linuxbrew`. This will give a false
/// negative if the if the user has changed the homebrew path.
#[cfg(target_os = "linux")]
#[inline]
fn installed_through_homebrew() -> bool {
    env::current_exe()
        .map(|path| path.starts_with("/home/linuxbrew/.linuxbrew"))
        .unwrap_or(false)
}

/// A naive way of checking if fp is installed through homebrew.
///
/// This will check if the current executable is located in the default macOS
/// homebrew location: `/usr/local` or `/opt/homebrew`. This will give a false
/// negative if the user has changed the homebrew path.
#[cfg(target_os = "macos")]
#[inline]
fn installed_through_homebrew() -> bool {
    env::current_exe()
        .map(|path| path.starts_with("/usr/local") || path.starts_with("/opt/homebrew"))
        .unwrap_or(false)
}

/// A naive way of checking if fp is installed through homebrew.
///
/// This target OS is not supported by homebrew. So we will just always return
/// false.
#[cfg(all(not(target_os = "linux"), not(target_os = "macos")))]
#[inline(always)]
fn installed_through_homebrew() -> bool {
    false
}
