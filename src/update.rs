use crate::{retrieve_latest_version, MANIFEST};
use anyhow::Result;
use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use std::io::{BufWriter, Write};
use std::os::unix::prelude::OpenOptionsExt;
use tracing::info;

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
    let mut res = reqwest::get(format!("https://fp.dev/fp/latest/{arch}/fp")).await?;
    let total_size = res.content_length().unwrap();

    let pb = ProgressBar::new(total_size);
    pb.set_style(ProgressStyle::default_bar()
        .template("{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}, {eta})")
        .progress_chars("#>-"));

    // Write the chunks to the buffered writer, while updating the progress-bar.
    let mut downloaded = 0;
    while let Some(chunk) = res.chunk().await? {
        temp_file.write_all(&chunk)?;

        downloaded += chunk.len();
        pb.set_position(downloaded as u64);
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
