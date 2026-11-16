use ed25519_dalek::{Signer, SigningKey};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const USAGE: &str = "Usage:
  zagcam-sign-manifest keygen <secret-key-path>
  zagcam-sign-manifest sign <secret-key-path> <manifest> [signature-path]

The signature is a raw 64-byte Ed25519 signature over the exact manifest bytes.
";

fn main() -> ExitCode {
    match run(std::env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(why) => {
            eprintln!("zagcam-sign-manifest: {why}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Vec<String>) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("keygen") if args.len() == 2 => keygen(Path::new(&args[1])),
        Some("sign") if args.len() == 3 || args.len() == 4 => sign(
            Path::new(&args[1]),
            Path::new(&args[2]),
            args.get(3).map(PathBuf::from),
        ),
        _ => Err(USAGE.into()),
    }
}

fn keygen(path: &Path) -> Result<(), String> {
    if path.exists() {
        return Err(format!("{} already exists", path.display()));
    }
    let mut seed = [0u8; 32];
    getrandom::getrandom(&mut seed).map_err(|why| format!("random key generation failed: {why}"))?;
    let key = SigningKey::from_bytes(&seed);
    if let Some(parent) = path.parent().filter(|parent| !parent.as_os_str().is_empty()) {
        fs::create_dir_all(parent)
            .map_err(|why| format!("creating {} failed: {why}", parent.display()))?;
    }
    fs::write(path, seed).map_err(|why| format!("writing {} failed: {why}", path.display()))?;

    let public = key.verifying_key().to_bytes();
    println!("secret key: {}", path.display());
    println!("public key: {}", hex::encode(public));
    println!("rust constant:");
    println!("pub const UPDATE_PUBLIC_KEY: [u8; 32] = [");
    for chunk in public.chunks(8) {
        let values = chunk
            .iter()
            .map(|byte| format!("0x{byte:02x}"))
            .collect::<Vec<_>>()
            .join(", ");
        println!("    {values},");
    }
    println!("];");
    Ok(())
}

fn sign(key_path: &Path, manifest: &Path, output: Option<PathBuf>) -> Result<(), String> {
    let seed = fs::read(key_path)
        .map_err(|why| format!("reading {} failed: {why}", key_path.display()))?;
    let seed: [u8; 32] = seed
        .try_into()
        .map_err(|_| format!("{} must contain exactly 32 bytes", key_path.display()))?;
    let key = SigningKey::from_bytes(&seed);
    let bytes =
        fs::read(manifest).map_err(|why| format!("reading {} failed: {why}", manifest.display()))?;
    let signature = key.sign(&bytes);
    let output = output.unwrap_or_else(|| PathBuf::from(format!("{}.sig", manifest.display())));
    fs::write(&output, signature.to_bytes())
        .map_err(|why| format!("writing {} failed: {why}", output.display()))?;
    println!("signature: {}", output.display());
    Ok(())
}