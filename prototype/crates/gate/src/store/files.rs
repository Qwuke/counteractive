//! The latest signed manifest, kept beside the store as plain files so it
//! can be served while Locked (docs 05 section 4).
//!
//! It isn't secret, and deleting it does nothing: wallets check its
//! signature, and the store's `manifest_version` is what the gate trusts.

use std::fs::{self, File};
use std::io::Write as _;
use std::path::Path;

use proto::{Blob, SignedManifest};

use super::sqlcipher::STORE_DIR;

const MANIFEST_FILE: &str = "manifest.json";
const SIGNATURE_FILE: &str = "manifest.sig";

/// The stored manifest, if both files are present and readable.
pub fn read_manifest(data: &Path) -> Option<SignedManifest> {
    let dir = data.join(STORE_DIR);
    let manifest = fs::read(dir.join(MANIFEST_FILE)).ok()?;
    let signature = fs::read_to_string(dir.join(SIGNATURE_FILE)).ok()?;
    Some(SignedManifest { manifest: Blob::new(manifest), signature })
}

/// Replaces the stored manifest. Each file is written to a temporary name,
/// synced and renamed, so a reader never sees a torn file.
pub fn write_manifest(data: &Path, signed: &SignedManifest) -> std::io::Result<()> {
    let dir = data.join(STORE_DIR);
    write_atomically(&dir, MANIFEST_FILE, signed.manifest.as_bytes())?;
    write_atomically(&dir, SIGNATURE_FILE, signed.signature.as_bytes())?;
    File::open(&dir)?.sync_all()
}

fn write_atomically(dir: &Path, name: &str, bytes: &[u8]) -> std::io::Result<()> {
    let staged = dir.join(format!("{name}.new"));
    let mut file = File::create(&staged)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    fs::rename(&staged, dir.join(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_replaces() {
        let dir = tempfile::tempdir().unwrap();
        assert!(read_manifest(dir.path()).is_none());
        fs::create_dir(dir.path().join(STORE_DIR)).unwrap();
        let first = SignedManifest { manifest: Blob::new(b"{\"v\":1}".to_vec()), signature: "sig one".into() };
        write_manifest(dir.path(), &first).unwrap();
        assert_eq!(read_manifest(dir.path()).unwrap(), first);
        let second = SignedManifest { manifest: Blob::new(b"{\"v\":2}".to_vec()), signature: "sig two".into() };
        write_manifest(dir.path(), &second).unwrap();
        assert_eq!(read_manifest(dir.path()).unwrap(), second);
    }
}
