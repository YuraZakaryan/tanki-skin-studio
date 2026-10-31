// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! HTTPS to GitHub: the API answer about the latest release, the small checksum file, and the
//! download of a release file. TLS goes through rustls with the system's certificates, and the
//! system's proxy settings apply (`HTTPS_PROXY`, and on Windows the Internet Options).

use std::{
    fs::File,
    io::{Read, Write},
    path::Path,
    time::Duration,
};

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};
use ureq::{
    Agent,
    tls::{RootCerts, TlsConfig},
};

use crate::progress::UpdateProgress;

/// The most an API answer or checksum file may be: the latest release's JSON is a few kilobytes.
const MAX_TEXT: u64 = 1 << 20;

fn agent() -> Agent {
    let tls = TlsConfig::builder()
        .root_certs(RootCerts::PlatformVerifier)
        .build();
    Agent::config_builder()
        .user_agent(format!("TankiSkinStudio/{}", env!("CARGO_PKG_VERSION")))
        .tls_config(tls)
        .http_status_as_error(false)
        // Redirects (to GitHub's download host) stay on HTTPS.
        .https_only(true)
        .timeout_resolve(Some(Duration::from_secs(15)))
        .timeout_connect(Some(Duration::from_secs(15)))
        // A download that stalls for a minute is given up rather than waited on forever.
        .timeout_per_read(Some(Duration::from_secs(60)))
        .build()
        .into()
}

fn unreachable(error: ureq::Error) -> anyhow::Error {
    anyhow::anyhow!("could not reach GitHub ({error})")
}

/// The text GitHub's REST API answers at `url`; `None` when it has nothing there (404).
pub fn get_api(url: &str) -> Result<Option<String>> {
    let mut response = agent()
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .call()
        .map_err(unreachable)?;
    let status = response.status().as_u16();
    let exhausted = response
        .headers()
        .get("x-ratelimit-remaining")
        .is_some_and(|v| v == "0");
    match status {
        200 => {}
        404 => return Ok(None),
        403 | 429 if status == 429 || exhausted => {
            bail!("GitHub's limit of requests is reached for now; try again in an hour")
        }
        _ => bail!("GitHub answered {status}"),
    }
    let text = response
        .body_mut()
        .with_config()
        .limit(MAX_TEXT)
        .read_to_string()
        .map_err(unreachable)?;
    Ok(Some(text))
}

/// A small text file of a release (its checksums).
pub fn get_text(url: &str) -> Result<String> {
    let mut response = agent().get(url).call().map_err(unreachable)?;
    let status = response.status().as_u16();
    anyhow::ensure!(status == 200, "GitHub answered {status} for {url}");
    response
        .body_mut()
        .with_config()
        .limit(MAX_TEXT)
        .read_to_string()
        .map_err(unreachable)
}

/// Download `url`, which must be `size` bytes, to `dest`, counting into `progress` and stopping
/// when it is cancelled. Returns the file's SHA-256 (lowercase hex). The file appears at `dest`
/// only when it is complete; a failed or cancelled download leaves nothing.
pub fn download(url: &str, dest: &Path, size: u64, progress: &UpdateProgress) -> Result<String> {
    let part = dest.with_extension("part");
    let result = download_to(url, &part, size, progress);
    match result {
        Ok(hash) => {
            std::fs::rename(&part, dest)
                .with_context(|| format!("could not save {}", dest.display()))?;
            Ok(hash)
        }
        Err(error) => {
            let _ = std::fs::remove_file(&part);
            Err(error)
        }
    }
}

fn download_to(url: &str, part: &Path, size: u64, progress: &UpdateProgress) -> Result<String> {
    let mut response = agent().get(url).call().map_err(unreachable)?;
    let status = response.status().as_u16();
    anyhow::ensure!(status == 200, "GitHub answered {status} for the download");
    progress.set_total(size);
    let mut file =
        File::create(part).with_context(|| format!("could not write {}", part.display()))?;
    let mut reader = response.body_mut().with_config().limit(size + 1).reader();
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024];
    let mut written = 0_u64;
    loop {
        progress.check()?;
        let read = reader
            .read(&mut buffer)
            .map_err(|e| anyhow::anyhow!("the download broke off ({e})"))?;
        if read == 0 {
            break;
        }
        written += read as u64;
        anyhow::ensure!(
            written <= size,
            "the download is larger than the release says"
        );
        hasher.update(&buffer[..read]);
        file.write_all(&buffer[..read])
            .with_context(|| format!("could not write {}", part.display()))?;
        progress.advance(read as u64);
    }
    anyhow::ensure!(written == size, "the download is incomplete");
    file.sync_all()?;
    Ok(hex(&hasher.finalize()))
}

/// Bytes as lowercase hex.
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_are_written_in_lowercase_hex() {
        assert_eq!(hex(&[0x00, 0xab, 0x0f]), "00ab0f");
        assert_eq!(
            hex(&Sha256::digest(b"")),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
