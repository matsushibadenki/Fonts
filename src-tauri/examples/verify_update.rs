//! Independently validate a release archive with the application's public key.
use base64::Engine;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 3 {
        return Err("usage: verify_update ARCHIVE SIGNATURE TAURI_CONFIG".into());
    }
    let config: serde_json::Value = serde_json::from_slice(&std::fs::read(&args[2])?)?;
    let key = base64::engine::general_purpose::STANDARD.decode(
        config["plugins"]["updater"]["pubkey"]
            .as_str()
            .ok_or("missing public key")?,
    )?;
    let key = minisign_verify::PublicKey::decode(std::str::from_utf8(&key)?)?;
    let signature = base64::engine::general_purpose::STANDARD
        .decode(std::fs::read_to_string(&args[1])?.trim())?;
    let signature = minisign_verify::Signature::decode(std::str::from_utf8(&signature)?)?;
    let data = std::fs::read(&args[0])?;
    key.verify(&data, &signature, false)?;
    let version = config["version"].as_str().ok_or("missing version")?;
    if !signature
        .trusted_comment()
        .split('\t')
        .any(|v| v == format!("version:{version}"))
    {
        return Err("signature version does not match application version".into());
    }
    // A corrupted archive must fail with this same signature.
    let mut corrupt = data;
    if let Some(byte) = corrupt.first_mut() {
        *byte ^= 1;
    }
    if key.verify(&corrupt, &signature, false).is_ok() {
        return Err("corruption was not detected".into());
    }
    println!("Signature and signed version {version} verified; corrupted bytes rejected.");
    Ok(())
}
