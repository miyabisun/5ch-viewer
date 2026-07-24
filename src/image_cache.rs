use image::{AnimationDecoder, ImageDecoder, ImageFormat, ImageReader, Limits};
use rusqlite::Connection;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufReader, Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub const SHARD_SIZE: i64 = 4096;
// A resized 1024x1024 RGBA8 pixel buffer is 4 MiB, so normalized output fits
// within the pre-existing 5 MiB cache and download boundary.
pub const MAX_IMAGE_BYTES: u64 = 5 * 1024 * 1024;
pub const MAX_IMAGE_DIMENSION: u32 = 16_384;
pub const MAX_DECODE_ALLOC: u64 = 128 * 1024 * 1024;
pub const RESIZE_BOUND: u32 = 1024;
static NEXT_TEMP_FILE: AtomicU64 = AtomicU64::new(1);

#[derive(Debug)]
pub struct ResizedImage {
    pub bytes: Vec<u8>,
    pub mime: String,
    pub resized: bool,
}

fn image_format(mime: &str) -> Option<ImageFormat> {
    match mime.split(';').next()?.trim().to_ascii_lowercase().as_str() {
        "image/png" => Some(ImageFormat::Png),
        "image/jpeg" => Some(ImageFormat::Jpeg),
        "image/gif" => Some(ImageFormat::Gif),
        "image/webp" => Some(ImageFormat::WebP),
        _ => None,
    }
}

fn decode_limits() -> Limits {
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_IMAGE_DIMENSION);
    limits.max_image_height = Some(MAX_IMAGE_DIMENSION);
    limits.max_alloc = Some(MAX_DECODE_ALLOC);
    limits
}

fn dimensions_with_limits(bytes: &[u8], format: ImageFormat) -> image::ImageResult<(u32, u32)> {
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    reader.limits(decode_limits());
    Ok(reader.into_decoder()?.dimensions())
}

fn is_animated(bytes: &[u8], format: ImageFormat) -> image::ImageResult<bool> {
    match format {
        ImageFormat::Gif => {
            let mut decoder = image::codecs::gif::GifDecoder::new(Cursor::new(bytes))?;
            decoder.set_limits(decode_limits())?;
            let mut frames = decoder.into_frames();
            let first = frames.next().transpose()?;
            let second = frames.next().transpose()?;
            Ok(first.is_some() && second.is_some())
        }
        ImageFormat::WebP => {
            let mut decoder =
                image::codecs::webp::WebPDecoder::new(BufReader::new(Cursor::new(bytes)))?;
            decoder.set_limits(decode_limits())?;
            Ok(decoder.has_animation())
        }
        ImageFormat::Png => {
            let decoder =
                image::codecs::png::PngDecoder::with_limits(Cursor::new(bytes), decode_limits())?;
            decoder.is_apng()
        }
        _ => Ok(false),
    }
}

fn try_resize_image(bytes: &[u8], format: ImageFormat) -> image::ImageResult<Option<Vec<u8>>> {
    let (width, height) = dimensions_with_limits(bytes, format)?;
    if width <= RESIZE_BOUND && height <= RESIZE_BOUND {
        return Ok(None);
    }
    if is_animated(bytes, format)? {
        return Ok(None);
    }

    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    reader.limits(decode_limits());
    let image = reader.decode()?;
    let resized = image.resize(
        RESIZE_BOUND,
        RESIZE_BOUND,
        image::imageops::FilterType::Lanczos3,
    );
    // Normalize high-bit-depth sources so the bounded pixel dimensions also
    // imply a bounded encoded cache size. JPEG has no alpha channel.
    let resized = if format == ImageFormat::Jpeg {
        image::DynamicImage::ImageRgb8(resized.to_rgb8())
    } else {
        image::DynamicImage::ImageRgba8(resized.to_rgba8())
    };
    let mut output = Cursor::new(Vec::new());
    resized.write_to(&mut output, format)?;
    Ok(Some(output.into_inner()))
}

/// Shrinks a supported still image to fit inside 1024x1024. Any unsupported,
/// animated, malformed, or resource-limit-exceeding input is returned byte-for-byte.
pub fn resize_image(bytes: Vec<u8>, mime: String) -> ResizedImage {
    let Some(format) = image_format(&mime) else {
        return ResizedImage {
            bytes,
            mime,
            resized: false,
        };
    };

    match try_resize_image(&bytes, format) {
        Ok(Some(output)) => ResizedImage {
            bytes: output,
            mime,
            resized: true,
        },
        Ok(None) => ResizedImage {
            bytes,
            mime,
            resized: false,
        },
        Err(error) => {
            tracing::warn!(%error, "image resize skipped; preserving original bytes");
            ResizedImage {
                bytes,
                mime,
                resized: false,
            }
        }
    }
}

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

fn validate_directory(path: &Path) -> io::Result<()> {
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

fn validate_replace_target(path: &Path) -> io::Result<()> {
    let meta = fs::symlink_metadata(path)?;
    if meta.file_type().is_file() && is_single_link(&meta) {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "cache replacement target is not a single-link regular file",
        ))
    }
}

/// Atomically replaces an existing cache file. Intended for offline maintenance;
/// unlike `write_verified`, this never creates the final cache entry.
pub fn replace_verified(root: &Path, id: i64, bytes: &[u8]) -> io::Result<()> {
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

    validate_replace_target(&final_path)?;
    let (temp_path, mut file) = create_temp_file(parent, id, std::process::id(), &NEXT_TEMP_FILE)?;
    let result = (|| {
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        // Recheck immediately before rename so symlinks and hard links are never replaced.
        validate_replace_target(&final_path)?;
        fs::rename(&temp_path, &final_path)?;
        if read_verified(root, id, bytes.len() as i64)? != bytes {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "cache replacement verification failed",
            ));
        }
        sync_parent(parent)
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
    validate_directory(root)?;
    let path = absolute_path(root, id);
    validate_directory(path.parent().expect("cache path has parent"))?;
    let meta = fs::symlink_metadata(path)?;
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
    validate_directory(root)?;
    validate_directory(path.parent().expect("cache path has parent"))?;
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

/// Reads a cache file using its on-disk size rather than SQLite metadata. This is
/// used by offline repair after a crash between atomic replacement and DB update.
pub fn read_current_verified(root: &Path, id: i64) -> io::Result<Vec<u8>> {
    let path = absolute_path(root, id);
    validate_directory(root)?;
    validate_directory(path.parent().expect("cache path has parent"))?;
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    }
    let mut file = options.open(path)?;
    let meta = file.metadata()?;
    if !meta.is_file() || !is_single_link(&meta) || meta.len() > MAX_IMAGE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "cached image is not a valid single-link regular file",
        ));
    }
    let mut bytes = Vec::with_capacity(meta.len() as usize);
    file.read_to_end(&mut bytes)?;
    let after = file.metadata()?;
    if bytes.len() as u64 != meta.len() || after.len() != meta.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "cached image changed while reading",
        ));
    }
    Ok(bytes)
}

#[derive(Debug, Default, Eq, PartialEq)]
pub struct BulkResizeReport {
    pub scanned: usize,
    pub resized: usize,
    pub metadata_repaired: usize,
    pub unchanged: usize,
    pub skipped_uncached: usize,
    pub errors: Vec<String>,
    pub diagnostics: Vec<String>,
}

/// Processes every filesystem-backed image cache row. Each file publication and
/// each metadata update is individually durable, making the operation rerunnable.
pub fn resize_cache(
    conn: &Connection,
    root: &Path,
    dry_run: bool,
) -> rusqlite::Result<BulkResizeReport> {
    let rows = {
        let mut stmt = conn.prepare("SELECT id, mime, file_size FROM image_cache ORDER BY id")?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    };

    let mut report = BulkResizeReport::default();
    for (id, mime, recorded_size) in rows {
        report.scanned += 1;
        let original = match read_current_verified(root, id) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound && recorded_size.is_none() => {
                report.skipped_uncached += 1;
                report
                    .diagnostics
                    .push(format!("id {id}: skipped (no cached file)"));
                continue;
            }
            Err(error) => {
                let message = format!("id {id}: read failed: {error}");
                report.errors.push(message.clone());
                report.diagnostics.push(message);
                continue;
            }
        };

        let transformed = resize_image(original, mime);
        let actual_size = transformed.bytes.len() as i64;
        let mut actions = Vec::new();
        if transformed.resized {
            if dry_run {
                report.resized += 1;
                actions.push(format!("would resize to {actual_size} bytes"));
            } else {
                match replace_verified(root, id, &transformed.bytes) {
                    Ok(()) => {
                        report.resized += 1;
                        actions.push(format!("resized to {actual_size} bytes"));
                    }
                    Err(error) => {
                        let message = format!("id {id}: replacement failed: {error}");
                        report.errors.push(message.clone());
                        report.diagnostics.push(message);
                        continue;
                    }
                }
            }
        } else {
            report.unchanged += 1;
            actions.push(format!("unchanged ({actual_size} bytes)"));
        }

        if recorded_size != Some(actual_size) {
            if dry_run {
                report.metadata_repaired += 1;
                actions.push(format!(
                    "would repair metadata {:?} -> {actual_size}",
                    recorded_size
                ));
            }
            if !dry_run {
                match conn.execute(
                    "UPDATE image_cache SET file_size=?1 WHERE id=?2",
                    rusqlite::params![actual_size, id],
                ) {
                    Ok(_) => {
                        report.metadata_repaired += 1;
                        actions.push(format!(
                            "repaired metadata {:?} -> {actual_size}",
                            recorded_size
                        ));
                    }
                    Err(error) => {
                        let message = format!("id {id}: metadata update failed: {error}");
                        report.errors.push(message.clone());
                        report.diagnostics.push(message);
                    }
                }
            }
        }
        report
            .diagnostics
            .push(format!("id {id}: {}", actions.join(", ")));
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{
        DynamicImage, Frame, GenericImageView, ImageBuffer, ImageFormat, Rgb, Rgba, RgbaImage,
    };
    use std::io::Cursor;
    use std::sync::{Arc, Barrier};

    fn test_root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("fivech-{name}-{}", std::process::id()))
    }

    fn encoded_image(width: u32, height: u32, format: ImageFormat) -> Vec<u8> {
        let image =
            DynamicImage::ImageRgb8(ImageBuffer::from_pixel(width, height, Rgb([23, 42, 99])));
        let mut bytes = Cursor::new(Vec::new());
        image.write_to(&mut bytes, format).unwrap();
        bytes.into_inner()
    }

    fn dimensions(bytes: &[u8], format: ImageFormat) -> (u32, u32) {
        image::load_from_memory_with_format(bytes, format)
            .unwrap()
            .dimensions()
    }

    #[test]
    fn resize_image_fits_landscape_inside_1024_square() {
        let original = encoded_image(2048, 512, ImageFormat::Png);
        let resized = resize_image(original, "image/png".into());
        assert!(resized.resized);
        assert_eq!(resized.mime, "image/png");
        assert_eq!(dimensions(&resized.bytes, ImageFormat::Png), (1024, 256));
    }

    #[test]
    fn resize_image_fits_portrait_inside_1024_square() {
        let original = encoded_image(512, 2048, ImageFormat::Jpeg);
        let resized = resize_image(original, "image/jpeg".into());
        assert!(resized.resized);
        assert_eq!(resized.mime, "image/jpeg");
        assert_eq!(dimensions(&resized.bytes, ImageFormat::Jpeg), (256, 1024));
    }

    #[test]
    fn resize_image_supports_static_gif_and_webp() {
        for (format, mime) in [
            (ImageFormat::Gif, "image/gif"),
            (ImageFormat::WebP, "image/webp"),
        ] {
            let original = encoded_image(1200, 300, format);
            let resized = resize_image(original, mime.into());
            assert!(resized.resized, "{mime} should be resized");
            assert_eq!(resized.mime, mime);
            assert_eq!(dimensions(&resized.bytes, format), (1024, 256));
        }
    }

    #[test]
    fn resize_image_keeps_resized_pixels_when_encoding_grows() {
        let original = encoded_image(1025, 1, ImageFormat::Png);
        let original_size = original.len();
        let resized = resize_image(original, "image/png".into());
        assert!(resized.resized);
        assert_eq!(dimensions(&resized.bytes, ImageFormat::Png), (1024, 1));
        assert!(
            resized.bytes.len() > original_size,
            "fixture must exercise a larger re-encode"
        );
    }

    #[test]
    fn resize_image_does_not_reencode_exact_or_small_images() {
        for (width, height, format, mime) in [
            (1024, 1024, ImageFormat::Png, "image/png"),
            (320, 240, ImageFormat::WebP, "image/webp"),
        ] {
            let original = encoded_image(width, height, format);
            let resized = resize_image(original.clone(), mime.into());
            assert!(!resized.resized);
            assert_eq!(resized.bytes, original);
            assert_eq!(resized.mime, mime);
        }
    }

    #[test]
    fn resize_image_preserves_malformed_input() {
        let original = b"not actually a gif".to_vec();
        let resized = resize_image(original.clone(), "image/gif".into());
        assert!(!resized.resized);
        assert_eq!(resized.bytes, original);
        assert_eq!(resized.mime, "image/gif");
    }

    #[test]
    fn resize_image_preserves_input_over_dimension_limit() {
        let original = encoded_image(MAX_IMAGE_DIMENSION + 1, 1, ImageFormat::Png);
        let resized = resize_image(original.clone(), "image/png".into());
        assert!(!resized.resized);
        assert_eq!(resized.bytes, original);
    }

    #[test]
    fn resize_image_preserves_animated_gif() {
        let mut original = Vec::new();
        {
            let mut encoder = image::codecs::gif::GifEncoder::new(&mut original);
            let frames = [[1, 2, 3, 255], [4, 5, 6, 255]]
                .map(|color| Frame::new(RgbaImage::from_pixel(1200, 2, Rgba(color))));
            encoder.encode_frames(frames).unwrap();
        }
        let resized = resize_image(original.clone(), "image/gif".into());
        assert!(!resized.resized);
        assert_eq!(resized.bytes, original);
    }

    #[test]
    fn bulk_resize_is_dry_runnable_rerunnable_and_repairs_metadata() {
        let root = test_root("bulk-resize");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir(&root).unwrap();
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE image_cache (
                id INTEGER PRIMARY KEY,
                mime TEXT NOT NULL,
                file_size INTEGER
            );",
        )
        .unwrap();

        let large = encoded_image(2048, 512, ImageFormat::Png);
        let small = encoded_image(320, 240, ImageFormat::WebP);
        let crash_original = encoded_image(512, 2048, ImageFormat::Jpeg);
        for (id, mime, bytes) in [
            (1, "image/png", large.as_slice()),
            (2, "image/webp", small.as_slice()),
            (3, "image/jpeg", crash_original.as_slice()),
        ] {
            conn.execute(
                "INSERT INTO image_cache (id, mime, file_size) VALUES (?1, ?2, 1)",
                rusqlite::params![id, mime],
            )
            .unwrap();
            write_verified(&root, id, bytes).unwrap();
        }

        // Simulate a crash after file replacement but before SQLite metadata update.
        let crash_resized = resize_image(crash_original, "image/jpeg".into());
        assert!(crash_resized.resized);
        replace_verified(&root, 3, &crash_resized.bytes).unwrap();

        let dry = resize_cache(&conn, &root, true).unwrap();
        assert_eq!(dry.resized, 1);
        assert_eq!(dry.metadata_repaired, 3);
        assert!(dry.errors.is_empty());
        assert_eq!(read_current_verified(&root, 1).unwrap(), large);
        assert_eq!(
            conn.query_row("SELECT file_size FROM image_cache WHERE id=2", [], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap(),
            1
        );

        let applied = resize_cache(&conn, &root, false).unwrap();
        assert_eq!(applied.resized, 1);
        assert_eq!(applied.metadata_repaired, 3);
        assert!(applied.errors.is_empty());
        assert_eq!(
            dimensions(&read_current_verified(&root, 1).unwrap(), ImageFormat::Png),
            (1024, 256)
        );
        assert_eq!(read_current_verified(&root, 2).unwrap(), small);
        for id in 1..=3 {
            let disk_size = read_current_verified(&root, id).unwrap().len() as i64;
            let db_size = conn
                .query_row("SELECT file_size FROM image_cache WHERE id=?1", [id], |r| {
                    r.get::<_, i64>(0)
                })
                .unwrap();
            assert_eq!(db_size, disk_size);
        }

        let rerun = resize_cache(&conn, &root, false).unwrap();
        assert_eq!(rerun.resized, 0);
        assert_eq!(rerun.metadata_repaired, 0);
        assert_eq!(rerun.unchanged, 3);
        assert!(rerun.errors.is_empty());
        let _ = fs::remove_dir_all(root);
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
        assert!(read_current_verified(&root, 1).is_err());
        assert!(replace_verified(&root, 1, b"abc").is_err());
        let _ = fs::remove_dir_all(root);
    }
}
