// SPDX-FileCopyrightText: 2026 Tanki Skin Studio contributors
// SPDX-License-Identifier: GPL-3.0-or-later
//! Checking a download: the SHA-256 the release's `SHA256SUMS.txt` lists for the file, which must
//! agree with GitHub's own digest of it when the API reports one. Nothing is installed that is not
//! listed.

use std::{io::Read, path::Path};

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};

use crate::release::Asset;

/// The name of the checksum file every release carries.
pub const SUMS: &str = "SHA256SUMS.txt";

/// Whether `text` is a SHA-256 in hex.
pub fn is_sha256(text: &str) -> bool {
    text.len() == 64 && text.bytes().all(|b| b.is_ascii_hexdigit())
}

/// The SHA-256 of `bytes`, lowercase hex.
pub fn sha256(bytes: &[u8]) -> String {
    crate::http::hex(&Sha256::digest(bytes))
}

/// The SHA-256 of the file at `path`, lowercase hex.
pub fn sha256_file(path: &Path) -> Result<String> {
    let mut file =
        std::fs::File::open(path).with_context(|| format!("could not read {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(crate::http::hex(&hasher.finalize()))
}

/// The SHA-256 (lowercase hex) that `sums`, in `sha256sum` format (`<hash>  <name>`, or
/// `<hash> *<name>` for binary mode), lists for the file `name`.
pub fn listed(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let (hash, file) = line.trim_end().split_once(' ')?;
        let file = file.strip_prefix([' ', '*']).unwrap_or(file);
        (file == name && is_sha256(hash)).then(|| hash.to_ascii_lowercase())
    })
}

/// The SHA-256 the download of `asset` must have: the one `sums` lists, which GitHub's digest,
/// when known, must match.
pub fn expected(sums: &str, asset: &Asset) -> Result<String> {
    let Some(hash) = listed(sums, &asset.name) else {
        bail!("{} is not listed in {SUMS}", asset.name);
    };
    if let Some(digest) = &asset.sha256
        && *digest != hash
    {
        bail!(
            "{SUMS} and GitHub disagree about the checksum of {}",
            asset.name
        );
    }
    Ok(hash)
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "0211011e05228b572a13604e370f25d07325cf2fcd74080161642ce5faad0195";
    const B: &str = "3a6d1e5e2186db01b9fadbadea2e4ac5683a6ab842a588136260d31967eb4d7b";

    fn asset(name: &str, sha256: Option<&str>) -> Asset {
        Asset {
            name: name.into(),
            url: String::new(),
            size: 1,
            sha256: sha256.map(str::to_owned),
        }
    }

    #[test]
    fn a_file_hashes_like_its_bytes() {
        let path = std::env::temp_dir().join(format!("tanki-verify-{}", std::process::id()));
        std::fs::write(&path, b"abc").unwrap();
        assert_eq!(sha256_file(&path).unwrap(), sha256(b"abc"));
        assert_eq!(
            sha256(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        std::fs::remove_file(path).unwrap();
        assert!(sha256_file(Path::new("/no/such/file")).is_err());
    }

    #[test]
    fn a_file_is_found_in_text_and_binary_mode_lines() {
        let sums = format!("{A}  app.dmg\n{} *app.msi\n", B.to_uppercase());
        assert_eq!(listed(&sums, "app.dmg").as_deref(), Some(A));
        assert_eq!(listed(&sums, "app.msi").as_deref(), Some(B));
        assert_eq!(listed(&sums, "app.zip"), None);
        assert_eq!(listed("nothing  app.dmg", "app.dmg"), None);
    }

    #[test]
    fn the_listed_checksum_must_agree_with_githubs() {
        let sums = format!("{A}  app.dmg\n");
        assert_eq!(expected(&sums, &asset("app.dmg", Some(A))).unwrap(), A);
        assert_eq!(expected(&sums, &asset("app.dmg", None)).unwrap(), A);
        assert!(expected(&sums, &asset("app.dmg", Some(B))).is_err());
        assert!(
            expected(&sums, &asset("app.msi", Some(B))).is_err(),
            "unlisted"
        );
    }
}
