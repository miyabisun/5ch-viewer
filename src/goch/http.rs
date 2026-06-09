//! HTTP access to 5ch.
//! - User-Agent is the client's default (contains Monazilla; state::USER_AGENT).
//! - Always `Accept-Encoding: identity` so compression doesn't break Content-Length / Range.
//! - Retry on 5xx, do not retry on 404.

use crate::error::AppError;
use crate::goch::dat::validate_diff;
use crate::goch::subject::{parse_subject_txt, SubjectEntry};
use crate::goch::url::validate_ref;
use reqwest::{Client, Response, StatusCode};
use std::time::Duration;

const MAX_ATTEMPTS: u32 = 4;
const RETRY_DELAY: Duration = Duration::from_millis(2000);
/// Number of trailing bytes used for boundary matching (Shift_JIS 2 bytes x 3 chars).
/// The dat always ends with a newline (0x0a), so 1 byte would collide. Verified on real servers.
const OVERLAP: u64 = 6;
/// 5ch host. Migrated from 5ch.net to 5ch.io in 2026-03 (the old net domain was revoked).
const HOST_SUFFIX: &str = "5ch.io";

/// Decodes a Shift_JIS byte sequence to UTF-8 (invalid bytes are replaced).
pub fn decode_shift_jis(bytes: &[u8]) -> String {
    let (cow, _, _) = encoding_rs::SHIFT_JIS.decode(bytes);
    cow.into_owned()
}

fn subject_url(server: &str, board: &str) -> String {
    format!("https://{server}.{HOST_SUFFIX}/{board}/subject.txt")
}
fn dat_url(server: &str, board: &str, thread_id: &str) -> String {
    format!("https://{server}.{HOST_SUFFIX}/{board}/dat/{thread_id}.dat")
}
fn setting_url(server: &str, board: &str) -> String {
    format!("https://{server}.{HOST_SUFFIX}/{board}/SETTING.TXT")
}

/// GET (fixed identity + optional Range). Retries on network errors and 5xx.
/// Returns 404/416/206/2xx as-is without retry (status handling is the caller's job).
async fn get(client: &Client, url: &str, range_from: Option<u64>) -> Result<Response, AppError> {
    let mut last = String::new();
    for attempt in 0..MAX_ATTEMPTS {
        let mut req = client.get(url).header("Accept-Encoding", "identity");
        if let Some(from) = range_from {
            req = req.header("Range", format!("bytes={from}-"));
        }
        match req.send().await {
            Ok(resp) => {
                if resp.status().is_server_error() && attempt < MAX_ATTEMPTS - 1 {
                    last = format!("HTTP {}", resp.status());
                    tokio::time::sleep(RETRY_DELAY).await;
                    continue;
                }
                return Ok(resp);
            }
            Err(e) => {
                last = e.to_string();
                if attempt < MAX_ATTEMPTS - 1 {
                    tokio::time::sleep(RETRY_DELAY).await;
                }
            }
        }
    }
    Err(AppError::Upstream(format!("GET failed: {url} ({last})")))
}

/// Fetches and parses subject.txt.
pub async fn fetch_subject(
    client: &Client,
    server: &str,
    board: &str,
) -> Result<Vec<SubjectEntry>, AppError> {
    // SSRF defense-in-depth: validate before assembling the URL (thread_id is unused, so a dummy).
    validate_ref(server, board, "0")?;
    let resp = get(client, &subject_url(server, board), None).await?;
    if !resp.status().is_success() {
        return Err(AppError::Upstream(format!("subject.txt HTTP {}", resp.status())));
    }
    let bytes = resp.bytes().await?;
    Ok(parse_subject_txt(&decode_shift_jis(&bytes)))
}

/// Fetches BBS_TITLE (the board's display name) from SETTING.TXT. Returns the board ID on failure.
pub async fn fetch_board_name(client: &Client, server: &str, board: &str) -> String {
    // SSRF defense-in-depth: on invalid server/board, fall back to the board ID without any request.
    if validate_ref(server, board, "0").is_err() {
        return board.to_string();
    }
    let resp = match get(client, &setting_url(server, board), None).await {
        Ok(r) if r.status().is_success() => r,
        _ => return board.to_string(),
    };
    let Ok(bytes) = resp.bytes().await else {
        return board.to_string();
    };
    let text = decode_shift_jis(&bytes);
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("BBS_TITLE=") {
            let name = rest.trim();
            if !name.is_empty() {
                return name.to_string();
            }
        }
    }
    board.to_string()
}

/// Result of a dat Range incremental fetch.
#[derive(Debug)]
pub enum DatFetch {
    /// 206: the increment (bytes to append at the end) and the new total byte count.
    Append { bytes: Vec<u8>, total: u64 },
    /// 200 or a refetch after detecting shrinkage: replace the whole body.
    Replace { bytes: Vec<u8>, total: u64 },
    /// 416 with the same size: no change.
    NotModified,
    /// 404: thread is gone.
    Gone,
}

/// Fetches the dat via a Range incremental request.
/// `from_bytes` is the previously stored Shift_JIS byte count; `last_tail` is up to OVERLAP
/// trailing bytes of the previous dat (for boundary matching). Corrupted diffs (deletions etc.) are repaired via a full fetch.
pub async fn fetch_dat(
    client: &Client,
    server: &str,
    board: &str,
    thread_id: &str,
    from_bytes: u64,
    last_tail: &[u8],
) -> Result<DatFetch, AppError> {
    // SSRF defense-in-depth: validate before assembling the URL.
    validate_ref(server, board, thread_id)?;
    let url = dat_url(server, board, thread_id);

    // First time: full fetch.
    if from_bytes == 0 {
        return refetch_full(client, &url).await;
    }

    // Fetch including the trailing overlap bytes to verify boundary match and diff validity.
    let overlap = OVERLAP.min(from_bytes).min(last_tail.len() as u64);
    let resp = get(client, &url, Some(from_bytes - overlap)).await?;

    match resp.status() {
        // The server ignored Range and returned the whole body.
        StatusCode::OK => {
            let bytes = resp.bytes().await?.to_vec();
            let total = bytes.len() as u64;
            Ok(DatFetch::Replace { bytes, total })
        }
        StatusCode::PARTIAL_CONTENT => {
            let bytes = resp.bytes().await?.to_vec();
            let o = overlap as usize;
            // (1) Boundary match: do the leading overlap bytes match the previous tail (deletion detection)?
            let boundary_ok =
                o == 0 || (bytes.len() >= o && bytes[..o] == last_tail[last_tail.len() - o..]);
            let appended = if bytes.len() >= o {
                bytes[o..].to_vec()
            } else {
                Vec::new()
            };
            // (2) Header validity of the diff (reject corrupted appends like the 5-byte problem).
            let diff_ok = validate_diff(&decode_shift_jis(&appended));
            if !boundary_ok || !diff_ok {
                tracing::info!("[dat] diff invalid (boundary={boundary_ok}, diff={diff_ok}) -> full refetch");
                return refetch_full(client, &url).await;
            }
            let total = from_bytes + appended.len() as u64;
            Ok(DatFetch::Append {
                bytes: appended,
                total,
            })
        }
        StatusCode::RANGE_NOT_SATISFIABLE => {
            // (4) Server-side size <= requested start position. Decide from Content-Range total.
            match content_range_total(&resp) {
                Some(t) if t == from_bytes => Ok(DatFetch::NotModified),
                _ => refetch_full(client, &url).await, // shrinkage (deletion)/unknown -> full fetch
            }
        }
        StatusCode::NOT_FOUND => Ok(DatFetch::Gone),
        s => Err(AppError::Upstream(format!("dat HTTP {s}"))),
    }
}

/// Refetches the entire dat (repair when the diff was corrupted, or initial fetch).
async fn refetch_full(client: &Client, url: &str) -> Result<DatFetch, AppError> {
    let resp = get(client, url, None).await?;
    match resp.status() {
        StatusCode::NOT_FOUND => Ok(DatFetch::Gone),
        s if s.is_success() => {
            let bytes = resp.bytes().await?.to_vec();
            let total = bytes.len() as u64;
            Ok(DatFetch::Replace { bytes, total })
        }
        s => Err(AppError::Upstream(format!("dat refetch HTTP {s}"))),
    }
}

/// Extracts the total size from the Content-Range header ("bytes */1234" or "bytes 0-1/1234").
fn content_range_total(resp: &Response) -> Option<u64> {
    let v = resp.headers().get(reqwest::header::CONTENT_RANGE)?;
    v.to_str().ok()?.rsplit('/').next()?.trim().parse().ok()
}
