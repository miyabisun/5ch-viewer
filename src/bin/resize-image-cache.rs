use rusqlite::{Connection, OpenFlags};
use std::path::Path;
use std::time::Duration;
use viewer_of_5ch::{config::Config, image_cache};

type AnyError = Box<dyn std::error::Error>;

fn usage() {
    println!(
        "Usage: resize-image-cache [--dry-run]\n\
         \n\
         Resize filesystem-backed image_cache entries while the application is stopped.\n\
         DATABASE_PATH and IMAGE_CACHE_DIR use the same environment variables as viewer-of-5ch."
    );
}

fn parse_args() -> Result<bool, AnyError> {
    let mut dry_run = false;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--dry-run" if !dry_run => dry_run = true,
            "-h" | "--help" => {
                usage();
                std::process::exit(0);
            }
            _ => return Err(format!("unknown argument: {arg}").into()),
        }
    }
    Ok(dry_run)
}

fn main() -> Result<(), AnyError> {
    dotenvy::dotenv().ok();
    let dry_run = parse_args()?;
    let config = Config::from_env();
    let conn = Connection::open_with_flags(&config.db_path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
    conn.busy_timeout(Duration::from_secs(5))?;
    let report = image_cache::resize_cache(&conn, Path::new(&config.image_cache_dir), dry_run)?;

    for line in &report.diagnostics {
        println!("{line}");
    }
    println!(
        "{}: scanned={}, resized={}, metadata_repairs={}, unchanged={}, uncached={}, errors={}",
        if dry_run { "dry-run" } else { "completed" },
        report.scanned,
        report.resized,
        report.metadata_repaired,
        report.unchanged,
        report.skipped_uncached,
        report.errors.len()
    );

    if report.errors.is_empty() {
        Ok(())
    } else {
        Err(format!("{} image cache row(s) failed", report.errors.len()).into())
    }
}
