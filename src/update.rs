//! Signed update manifest and installer download.

use ed25519_dalek::{Signature, VerifyingKey};
use semver::Version;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const DEFAULT_FEED: &str =
    "https://github.com/MahzuzH/ZagCam/releases/latest/download/latest.json";

/// Pinned key for signed update manifests; the private key stays offline.
pub const UPDATE_PUBLIC_KEY: [u8; 32] = [
    0x72, 0x83, 0x1f, 0x5d, 0xca, 0x96, 0xa5, 0x17, 0xed, 0xd8, 0x6c, 0x70, 0xb7, 0xd3, 0x3f, 0x64,
    0x6a, 0x2c, 0x49, 0x05, 0x1d, 0x2b, 0x2e, 0x9d, 0x6c, 0x9b, 0x6e, 0xdc, 0x11, 0x87, 0xb1, 0xf6,
];

const MAX_MANIFEST_BYTES: u64 = 64 * 1024;
const MAX_SIGNATURE_BYTES: u64 = 1024;
const MAX_INSTALLER_BYTES: u64 = 64 * 1024 * 1024;

const USAGE: &str = "Usage:
  zagcam update check [--feed URL|PATH]
";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Update {
    pub version: Version,
    pub url: String,
    pub sha256: String,
    pub notes_url: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Check {
    UpToDate { current: Version, latest: Version },
    Available(Update),
}

pub fn run(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("check") => {
            let feed = feed_from_args(&args[1..])?;
            match check(&feed)? {
                Check::UpToDate { current, latest } => {
                    println!("ZagCam {current} is up to date (latest {latest})");
                }
                Check::Available(update) => {
                    println!(
                        "ZagCam {} is available (current {}): {}",
                        update.version,
                        env!("CARGO_PKG_VERSION"),
                        update.url
                    );
                    if let Some(notes) = update.notes_url {
                        println!("notes: {notes}");
                    }
                }
            }
            Ok(())
        }
        Some("help" | "--help" | "-h") | None => {
            println!("{USAGE}");
            Ok(())
        }
        Some(other) => Err(format!("unknown update command {other}\n\n{USAGE}")),
    }
}

pub fn feed_url() -> String {
    std::env::var("ZAGCAM_UPDATE_FEED").unwrap_or_else(|_| DEFAULT_FEED.to_string())
}

fn feed_from_args(args: &[String]) -> Result<String, String> {
    let mut feed = feed_url();
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--feed" | "-f" => {
                feed = iter.next().ok_or("--feed needs a value")?.clone();
            }
            other => return Err(format!("unknown argument {other}\n\n{USAGE}")),
        }
    }
    Ok(feed)
}

pub fn check(feed: &str) -> Result<Check, String> {
    let manifest = fetch(feed, MAX_MANIFEST_BYTES)?;
    let signature = fetch(&format!("{feed}.sig"), MAX_SIGNATURE_BYTES)?;
    verify_signature(&manifest, &signature, &UPDATE_PUBLIC_KEY)?;

    let update = parse_manifest(&manifest)?;
    let current = Version::parse(env!("CARGO_PKG_VERSION"))
        .map_err(|why| format!("cannot parse the current version: {why}"))?;
    if update.version > current {
        Ok(Check::Available(update))
    } else {
        Ok(Check::UpToDate {
            current,
            latest: update.version,
        })
    }
}

pub fn download(update: &Update) -> Result<PathBuf, String> {
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(120)))
        .build();
    let agent: ureq::Agent = config.into();
    let mut response = agent
        .get(&update.url)
        .call()
        .map_err(|why| format!("cannot download {}: {why}", update.url))?;

    let dir = std::env::temp_dir().join("ZagCam").join("updates");
    fs::create_dir_all(&dir).map_err(|why| format!("creating {} failed: {why}", dir.display()))?;
    let target = dir.join(format!("ZagCam-{}.msi", update.version));
    let temp = dir.join(format!("ZagCam-{}.msi.part", update.version));
    let _ = fs::remove_file(&temp);

    let mut reader = response
        .body_mut()
        .with_config()
        .limit(MAX_INSTALLER_BYTES + 1)
        .reader();
    let mut file =
        fs::File::create(&temp).map_err(|why| format!("creating {} failed: {why}", temp.display()))?;
    let copied = std::io::copy(&mut reader, &mut file)
        .map_err(|why| format!("downloading {} failed: {why}", update.url))?;
    drop(file);
    if copied > MAX_INSTALLER_BYTES {
        let _ = fs::remove_file(&temp);
        return Err("update installer is larger than the 64 MiB limit".into());
    }

    let bytes = fs::read(&temp).map_err(|why| format!("reading {} failed: {why}", temp.display()))?;
    if let Err(why) = verify_sha256(&bytes, &update.sha256) {
        let _ = fs::remove_file(&temp);
        return Err(why);
    }

    let _ = fs::remove_file(&target);
    fs::rename(&temp, &target)
        .map_err(|why| format!("moving {} failed: {why}", target.display()))?;
    Ok(target)
}

pub fn install(msi: &Path) -> Result<(), String> {
    let params = format!("/i \"{}\" /passive /norestart", msi.display());
    crate::vcam::run_elevated(Path::new("msiexec.exe"), &params)
}

fn fetch(location: &str, limit: u64) -> Result<Vec<u8>, String> {
    if location.starts_with("https://") {
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(30)))
            .build();
        let agent: ureq::Agent = config.into();
        let mut response = agent
            .get(location)
            .call()
            .map_err(|why| format!("cannot fetch {location}: {why}"))?;
        return response
            .body_mut()
            .with_config()
            .limit(limit)
            .read_to_vec()
            .map_err(|why| format!("cannot read {location}: {why}"));
    }
    if location.starts_with("http://") {
        return Err("update feed must use HTTPS".into());
    }
    fs::read(location).map_err(|why| format!("cannot read {location}: {why}"))
}

fn parse_manifest(bytes: &[u8]) -> Result<Update, String> {
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|why| format!("invalid update manifest: {why}"))?;
    let object = value
        .as_object()
        .ok_or("update manifest must be a JSON object")?;
    let version = object
        .get("version")
        .and_then(serde_json::Value::as_str)
        .ok_or("update manifest has no version")?;
    let version = Version::parse(version)
        .map_err(|why| format!("invalid update version {version}: {why}"))?;
    let url = object
        .get("url")
        .and_then(serde_json::Value::as_str)
        .ok_or("update manifest has no URL")?;
    if !url.starts_with("https://") {
        return Err("update URL must use HTTPS".into());
    }
    let sha256 = object
        .get("sha256")
        .and_then(serde_json::Value::as_str)
        .ok_or("update manifest has no SHA-256")?;
    if sha256.len() != 64 || !sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("update manifest has an invalid SHA-256".into());
    }
    let notes_url = object
        .get("notes_url")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);
    if notes_url.as_deref().is_some_and(|url| !url.starts_with("https://")) {
        return Err("update notes URL must use HTTPS".into());
    }
    Ok(Update {
        version,
        url: url.to_string(),
        sha256: sha256.to_ascii_lowercase(),
        notes_url,
    })
}

fn verify_sha256(bytes: &[u8], expected: &str) -> Result<(), String> {
    let actual = hex::encode(Sha256::digest(bytes));
    if actual.eq_ignore_ascii_case(expected) {
        Ok(())
    } else {
        Err(format!(
            "update hash mismatch: expected {expected}, got {actual}"
        ))
    }
}

fn verify_signature(bytes: &[u8], signature: &[u8], key: &[u8; 32]) -> Result<(), String> {
    let signature: [u8; 64] = signature
        .try_into()
        .map_err(|_| "update signature must contain exactly 64 bytes".to_string())?;
    let key = VerifyingKey::from_bytes(key).map_err(|why| format!("invalid update key: {why}"))?;
    key.verify_strict(bytes, &Signature::from_bytes(&signature))
        .map_err(|_| "update signature is invalid".to_string())
}

#[cfg(test)]
mod tests {
    use super::{parse_manifest, verify_sha256, verify_signature};
    use ed25519_dalek::{Signer, SigningKey};
    use sha2::Digest;

    const KEY: [u8; 32] = [7; 32];

    fn signed(bytes: &[u8]) -> ([u8; 32], [u8; 64]) {
        let key = SigningKey::from_bytes(&KEY);
        (key.verifying_key().to_bytes(), key.sign(bytes).to_bytes())
    }

    fn manifest(version: &str, url: &str) -> Vec<u8> {
        format!(
            r#"{{"version":"{version}","url":"{url}","sha256":"{}"}}"#,
            "a".repeat(64)
        )
        .into_bytes()
    }

    #[test]
    fn valid_manifest_and_signature_are_accepted() {
        let bytes = manifest("0.6.0", "https://example.test/ZagCam.msi");
        let (key, signature) = signed(&bytes);
        verify_signature(&bytes, &signature, &key).unwrap();
        assert_eq!(parse_manifest(&bytes).unwrap().version.to_string(), "0.6.0");
    }

    #[test]
    fn changed_manifest_or_signature_is_rejected() {
        let bytes = manifest("0.6.0", "https://example.test/ZagCam.msi");
        let (key, signature) = signed(&bytes);
        let mut changed = bytes.clone();
        changed[0] = b' ';
        assert!(verify_signature(&changed, &signature, &key).is_err());

        let mut bad_signature = signature;
        bad_signature[0] ^= 1;
        assert!(verify_signature(&bytes, &bad_signature, &key).is_err());
    }

    #[test]
    fn manifest_rejects_http_and_bad_hashes() {
        let http = manifest("0.6.0", "http://example.test/ZagCam.msi");
        assert!(parse_manifest(&http).is_err());
        let bad_hash = br#"{"version":"0.6.0","url":"https://example.test/ZagCam.msi","sha256":"abc"}"#;
        assert!(parse_manifest(bad_hash).is_err());
    }

    #[test]
    fn download_hash_must_match() {
        let bytes = b"installer";
        assert!(verify_sha256(bytes, &hex::encode(sha2::Sha256::digest(bytes))).is_ok());
        assert!(verify_sha256(bytes, &"a".repeat(64)).is_err());
    }
}