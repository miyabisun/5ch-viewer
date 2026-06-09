//! HTTP access to 5ch.
//! - User-Agent is the client's default (contains Monazilla; state::USER_AGENT).
//! - Always `Accept-Encoding: identity` so compression doesn't break Content-Length.
//! - Retry on 5xx, do not retry on 404.

use crate::error::AppError;
use crate::goch::subject::{parse_subject_txt, SubjectEntry};
use crate::goch::url::validate_ref;
use reqwest::{Client, Response, StatusCode};
use std::time::Duration;

const MAX_ATTEMPTS: u32 = 4;
const RETRY_DELAY: Duration = Duration::from_millis(2000);
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

/// GET (fixed identity). Retries on network errors and 5xx.
/// Returns 404/2xx as-is without retry (status handling is the caller's job).
async fn get(client: &Client, url: &str) -> Result<Response, AppError> {
    let mut last = String::new();
    for attempt in 0..MAX_ATTEMPTS {
        let req = client.get(url).header("Accept-Encoding", "identity");
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
    let resp = get(client, &subject_url(server, board)).await?;
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
    let resp = match get(client, &setting_url(server, board)).await {
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

/// Result of a full dat fetch.
#[derive(Debug)]
pub enum DatFetch {
    /// 2xx: the whole dat body and its byte count (always a full replace).
    Replace { bytes: Vec<u8>, total: u64 },
    /// 404: thread is gone.
    Gone,
}

/// Fetches the entire dat (no Range / no diff).
/// Returns the full body on success, Gone on 404. Take-or-skip is decided by the caller
/// (it checks subject.txt's res_count before calling this, so 5ch is not hit needlessly).
pub async fn fetch_dat(
    client: &Client,
    server: &str,
    board: &str,
    thread_id: &str,
) -> Result<DatFetch, AppError> {
    // SSRF defense-in-depth: validate before assembling the URL.
    validate_ref(server, board, thread_id)?;
    let url = dat_url(server, board, thread_id);
    let resp = get(client, &url).await?;
    match resp.status() {
        StatusCode::NOT_FOUND => Ok(DatFetch::Gone),
        s if s.is_success() => {
            let bytes = resp.bytes().await?.to_vec();
            let total = bytes.len() as u64;
            Ok(DatFetch::Replace { bytes, total })
        }
        s => Err(AppError::Upstream(format!("dat HTTP {s}"))),
    }
}
