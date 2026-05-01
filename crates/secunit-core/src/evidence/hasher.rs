//! SHA-256 over files and serialized JSON, plus atomic write helpers.
//!
//! Hashing is integrity-critical (`docs/storage.md`); aim for
//! 100% test coverage. The atomic-write helper writes to a sibling
//! `.tmp` file, fsyncs, and renames into place, so a crash mid-finalize
//! never leaves a half-written manifest.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use walkdir::WalkDir;

/// Compute the SHA-256 of a file's bytes. Streams 64 KiB chunks so it
/// works on large captures without buffering.
pub fn sha256_file(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

/// Compute the SHA-256 of an in-memory byte slice. Used for hashing the
/// canonical-JSON form of a manifest before writing it to disk.
pub fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

/// Walk a directory and return `(relative_path, sha256, byte_count)` for
/// every regular file found. Paths are returned in deterministic
/// lexicographic order so manifests are stable across re-runs.
///
/// Files whose names appear in `exclude_top_level` are skipped **only at
/// the root of the walk** — used to keep `manifest.json`, `prepare.json`,
/// `result.json`, and the `.run-pending` sentinel out of the artifact
/// list. A skill that writes a file with one of those names *under*
/// `by-system/<name>/raw/` still gets it hashed; the exclusion is
/// metadata-only, not magical.
pub fn hash_tree(root: &Path, exclude_top_level: &[&str]) -> io::Result<Vec<HashedArtifact>> {
    let mut entries: Vec<HashedArtifact> = Vec::new();
    for entry in WalkDir::new(root).sort_by_file_name() {
        let entry = entry.map_err(io::Error::other)?;
        if !entry.file_type().is_file() {
            continue;
        }
        let abs = entry.path();
        let is_top_level = abs.parent() == Some(root);
        if is_top_level {
            if let Some(name) = entry.file_name().to_str() {
                if exclude_top_level.contains(&name) {
                    continue;
                }
            }
        }
        let rel = abs
            .strip_prefix(root)
            .map_err(|e| io::Error::other(format!("strip_prefix: {e}")))?
            .to_path_buf();
        // Force forward slashes in manifest paths regardless of host.
        let rel_str = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/");
        let sha = sha256_file(abs)?;
        let bytes = fs::metadata(abs)?.len();
        entries.push(HashedArtifact {
            path: rel_str,
            sha256: sha,
            bytes,
            absolute: abs.to_path_buf(),
        });
    }
    Ok(entries)
}

#[derive(Debug, Clone)]
pub struct HashedArtifact {
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
    pub absolute: PathBuf,
}

/// Atomic write: stream `bytes` into `<dest>.tmp`, fsync the file and the
/// parent directory, then rename over `dest`. On POSIX the rename is
/// atomic. The fsync of the parent directory is what guarantees that
/// the rename itself survives a crash; without it, the rename's metadata
/// can still be lost.
pub fn atomic_write(dest: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = dest
        .parent()
        .ok_or_else(|| io::Error::other("atomic_write: dest has no parent"))?;
    let tmp = parent.join(format!(
        ".{}.tmp",
        dest.file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("anonymous")
    ));

    {
        let mut f = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    fs::rename(&tmp, dest)?;
    // fsync the directory to durably commit the rename.
    if let Ok(dir) = File::open(parent) {
        let _ = dir.sync_all();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_known_vector() {
        // SHA-256("abc") = ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad
        assert_eq!(
            sha256_bytes(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn hash_tree_is_deterministic_and_sorted() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("sub")).unwrap();
        fs::write(dir.path().join("b.txt"), b"two").unwrap();
        fs::write(dir.path().join("a.txt"), b"one").unwrap();
        fs::write(dir.path().join("sub/c.txt"), b"three").unwrap();
        // Excluded file at the top level should be skipped.
        fs::write(dir.path().join("manifest.json"), b"meta").unwrap();

        let result = hash_tree(dir.path(), &["manifest.json"]).unwrap();
        let paths: Vec<_> = result.iter().map(|h| h.path.clone()).collect();
        assert_eq!(paths, vec!["a.txt", "b.txt", "sub/c.txt"]);
        assert_eq!(result[0].sha256, sha256_bytes(b"one"));
        assert_eq!(result[1].sha256, sha256_bytes(b"two"));
        assert_eq!(result[2].sha256, sha256_bytes(b"three"));
        assert_eq!(result[0].bytes, 3);
        assert_eq!(result[2].bytes, 5);
    }

    #[test]
    fn hash_tree_excludes_only_at_top_level() {
        // A file with the same name as a metadata exclusion, but nested
        // under by-system/, must still be hashed — the exclusion is for
        // run-dir metadata only, not magical anywhere it appears.
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("by-system/foo/raw")).unwrap();
        fs::write(dir.path().join("manifest.json"), b"top-meta").unwrap();
        fs::write(
            dir.path().join("by-system/foo/raw/manifest.json"),
            b"nested",
        )
        .unwrap();

        let result = hash_tree(dir.path(), &["manifest.json"]).unwrap();
        let paths: Vec<_> = result.iter().map(|h| h.path.clone()).collect();
        assert_eq!(paths, vec!["by-system/foo/raw/manifest.json"]);
        assert_eq!(result[0].sha256, sha256_bytes(b"nested"));
    }

    #[test]
    fn atomic_write_replaces_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("out.json");
        atomic_write(&dest, b"first").unwrap();
        assert_eq!(fs::read(&dest).unwrap(), b"first");
        atomic_write(&dest, b"second").unwrap();
        assert_eq!(fs::read(&dest).unwrap(), b"second");
        // No leftover .tmp.
        let leftovers: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| {
                e.file_name()
                    .to_str()
                    .map(|s| s.ends_with(".tmp"))
                    .unwrap_or(false)
            })
            .collect();
        assert!(leftovers.is_empty(), "leftover tmp files: {leftovers:?}");
    }
}
