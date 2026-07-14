use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub const SHARD_SIZE: i64 = 4096;
pub const MAX_IMAGE_BYTES: u64 = 5 * 1024 * 1024;
static NEXT_TEMP_FILE: AtomicU64 = AtomicU64::new(1);

/// Returns the stable, exFAT-safe relative path for a positive SQLite image id.
pub fn relative_path(id: i64) -> String {
    assert!(id > 0, "image cache id must be positive");
    format!("{:04x}/{:08x}", (id - 1) / SHARD_SIZE, id)
}

pub fn absolute_path(root: &Path, id: i64) -> PathBuf {
    root.join(relative_path(id))
}

fn ensure_directory(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_dir() => Ok(()),
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "cache path component is not a real directory",
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            match fs::create_dir(path) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error),
            }
            let meta = fs::symlink_metadata(path)?;
            if meta.file_type().is_dir() {
                Ok(())
            } else {
                Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "cache path component is not a real directory",
                ))
            }
        }
        Err(error) => Err(error),
    }
}

#[cfg(unix)]
fn is_single_link(meta: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    meta.nlink() == 1
}

#[cfg(not(unix))]
fn is_single_link(_meta: &fs::Metadata) -> bool {
    true
}

fn existing_matches(path: &Path, bytes: &[u8]) -> io::Result<bool> {
    let meta = fs::symlink_metadata(path)?;
    if !meta.file_type().is_file() || !is_single_link(&meta) {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "cache path is not a single-link regular file",
        ));
    }
    Ok(fs::read(path)? == bytes)
}

fn create_temp_file(
    parent: &Path,
    id: i64,
    process_id: u32,
    sequence: &AtomicU64,
) -> io::Result<(PathBuf, File)> {
    for _ in 0..1024 {
        let sequence = sequence.fetch_add(1, Ordering::Relaxed);
        let path = parent.join(format!(".{id:08x}.{process_id}.{sequence}.tmp"));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not allocate a unique cache temporary file",
    ))
}

#[cfg(unix)]
struct ShardLock(File);

#[cfg(unix)]
impl ShardLock {
    fn acquire(parent: &Path) -> io::Result<Self> {
        use std::os::fd::AsRawFd;
        let file = File::open(parent)?;
        // Advisory locking is shared by every app/migration writer and released on crash.
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) } == 0 {
            Ok(Self(file))
        } else {
            Err(io::Error::last_os_error())
        }
    }
}

#[cfg(unix)]
impl Drop for ShardLock {
    fn drop(&mut self) {
        use std::os::fd::AsRawFd;
        let _ = unsafe { libc::flock(self.0.as_raw_fd(), libc::LOCK_UN) };
    }
}

#[cfg(not(unix))]
static SHARD_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Writes and syncs a private temporary file, then atomically publishes it while
/// holding the shard lock. Metadata is published only after this returns.
pub fn write_verified(root: &Path, id: i64, bytes: &[u8]) -> io::Result<()> {
    if bytes.len() as u64 > MAX_IMAGE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "image exceeds cache size limit",
        ));
    }

    ensure_directory(root)?;
    let final_path = absolute_path(root, id);
    let parent = final_path.parent().expect("cache path has parent");
    ensure_directory(parent)?;

    #[cfg(unix)]
    let _lock = ShardLock::acquire(parent)?;
    #[cfg(not(unix))]
    let _lock = SHARD_LOCK
        .lock()
        .map_err(|_| io::Error::other("cache lock poisoned"))?;

    match existing_matches(&final_path, bytes) {
        Ok(true) => {
            sync_parent(parent)?;
            return Ok(());
        }
        Ok(false) => {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "cache file content differs",
            ));
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }

    let (temp_path, mut file) = create_temp_file(parent, id, std::process::id(), &NEXT_TEMP_FILE)?;

    let result = (|| {
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        match existing_matches(&final_path, bytes) {
            Ok(true) => {
                sync_parent(parent)?;
                return Ok(());
            }
            Ok(false) => {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "cache file content differs",
                ));
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        fs::rename(&temp_path, &final_path)?;
        if read_verified(root, id, bytes.len() as i64)? != bytes {
            let _ = fs::remove_file(&final_path);
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "cache file verification failed",
            ));
        }
        sync_parent(parent)?;
        Ok(())
    })();
    let _ = fs::remove_file(&temp_path);
    result
}

#[cfg(unix)]
fn sync_parent(parent: &Path) -> io::Result<()> {
    File::open(parent)?.sync_all()
}

#[cfg(not(unix))]
fn sync_parent(_parent: &Path) -> io::Result<()> {
    Ok(())
}

pub fn audit_file(root: &Path, id: i64, file_size: Option<i64>) -> io::Result<bool> {
    let Some(expected) = file_size.filter(|size| *size >= 0) else {
        return Ok(false);
    };
    let meta = fs::symlink_metadata(absolute_path(root, id))?;
    Ok(meta.file_type().is_file()
        && is_single_link(&meta)
        && meta.len() == expected as u64
        && meta.len() <= MAX_IMAGE_BYTES)
}

/// Opens without following the final symlink on Unix and reads from that same handle.
pub fn read_verified(root: &Path, id: i64, expected_size: i64) -> io::Result<Vec<u8>> {
    if !(0..=MAX_IMAGE_BYTES as i64).contains(&expected_size) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid cached image size",
        ));
    }
    let path = absolute_path(root, id);
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    }
    let mut file = options.open(path)?;
    let meta = file.metadata()?;
    if !meta.is_file() || !is_single_link(&meta) || meta.len() != expected_size as u64 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "cached image metadata mismatch",
        ));
    }
    let mut bytes = Vec::with_capacity(expected_size as usize);
    file.read_to_end(&mut bytes)?;
    let after = file.metadata()?;
    if bytes.len() as i64 != expected_size || after.len() != meta.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "cached image changed while reading",
        ));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Barrier};

    fn test_root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("fivech-{name}-{}", std::process::id()))
    }

    #[test]
    fn shards_hold_at_most_4096_ids() {
        assert_eq!(relative_path(4095), "0000/00000fff");
        assert_eq!(relative_path(4096), "0000/00001000");
        assert_eq!(relative_path(4097), "0001/00001001");
        assert_eq!(relative_path(8192), "0001/00002000");
        assert_eq!(relative_path(8193), "0002/00002001");
    }

    #[test]
    fn existing_file_must_match_exactly() {
        let root = test_root("image-cache");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir(&root).unwrap();
        write_verified(&root, 1, b"abc").unwrap();
        write_verified(&root, 1, b"abc").unwrap();
        assert!(write_verified(&root, 1, b"xyz").is_err());
        assert_eq!(read_verified(&root, 1, 3).unwrap(), b"abc");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn concurrent_writers_never_replace_the_winner() {
        let root = test_root("image-cache-race");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir(&root).unwrap();
        let barrier = Arc::new(Barrier::new(3));
        let handles: Vec<_> = [b"abc".as_slice(), b"xyz".as_slice()]
            .into_iter()
            .map(|bytes| {
                let root = root.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    write_verified(&root, 1, bytes)
                })
            })
            .collect();
        barrier.wait();
        let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
        let bytes = read_verified(&root, 1, 3).unwrap();
        assert!(bytes == b"abc" || bytes == b"xyz");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn orphaned_partial_temp_file_does_not_block_retry() {
        let root = test_root("image-cache-retry");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir(&root).unwrap();
        let parent = absolute_path(&root, 1).parent().unwrap().to_path_buf();
        ensure_directory(&parent).unwrap();
        let sequence = AtomicU64::new(1);
        let collision = parent.join(format!(".00000001.{}.1.tmp", std::process::id()));
        fs::write(&collision, b"partial").unwrap();
        let (allocated, file) =
            create_temp_file(&parent, 1, std::process::id(), &sequence).unwrap();
        drop(file);
        assert!(allocated.ends_with(format!(".00000001.{}.2.tmp", std::process::id())));
        fs::remove_file(allocated).unwrap();
        write_verified(&root, 1, b"complete").unwrap();
        assert_eq!(read_verified(&root, 1, 8).unwrap(), b"complete");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn concurrent_writers_can_create_missing_directories() {
        let root = test_root("image-cache-directory-race");
        let _ = fs::remove_dir_all(&root);
        let barrier = Arc::new(Barrier::new(3));
        let handles: Vec<_> = [1, 2]
            .into_iter()
            .map(|id| {
                let root = root.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    write_verified(&root, id, b"abc")
                })
            })
            .collect();
        barrier.wait();
        for handle in handles {
            handle.join().unwrap().unwrap();
        }
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn symlink_is_not_reused_or_read() {
        let root = test_root("image-cache-symlink");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir(&root).unwrap();
        let target = root.join("target");
        fs::write(&target, b"abc").unwrap();
        ensure_directory(absolute_path(&root, 1).parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&target, absolute_path(&root, 1)).unwrap();
        assert!(write_verified(&root, 1, b"abc").is_err());
        assert!(read_verified(&root, 1, 3).is_err());
        let _ = fs::remove_dir_all(root);
    }
}
