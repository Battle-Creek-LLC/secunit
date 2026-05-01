//! Advisory file lock at `<root>/.secunit.lock`. Concurrent invocations
//! against the same root serialise on this lock so `state.json` writes
//! never interleave.

use std::fs::{File, OpenOptions};
use std::io;
use std::path::Path;

use fs2::FileExt;

pub struct RootLock {
    file: File,
}

impl RootLock {
    /// Acquire an exclusive lock at `<root>/.secunit.lock`. Blocks until
    /// the previous holder releases. Drop the returned guard to release.
    pub fn acquire(root: &Path) -> io::Result<Self> {
        let path = root.join(".secunit.lock");
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(&path)?;
        file.lock_exclusive()?;
        Ok(Self { file })
    }

    /// Try to acquire the lock without blocking. Returns `Ok(None)` if
    /// another process holds it.
    pub fn try_acquire(root: &Path) -> io::Result<Option<Self>> {
        let path = root.join(".secunit.lock");
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(&path)?;
        match file.try_lock_exclusive() {
            Ok(()) => Ok(Some(Self { file })),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => Ok(None),
            Err(e) => Err(e),
        }
    }
}

impl Drop for RootLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn try_acquire_returns_none_when_held() {
        let dir = tempfile::tempdir().unwrap();
        let _held = RootLock::acquire(dir.path()).unwrap();
        let again = RootLock::try_acquire(dir.path()).unwrap();
        assert!(again.is_none());
    }

    #[test]
    fn lock_is_released_on_drop() {
        let dir = tempfile::tempdir().unwrap();
        {
            let _held = RootLock::acquire(dir.path()).unwrap();
        }
        let again = RootLock::try_acquire(dir.path()).unwrap();
        assert!(again.is_some());
    }
}
