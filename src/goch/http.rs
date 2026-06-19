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
const HEAD_MAX_ATTEMPTS: u32 = 2; // HEAD failures fall back to full GET; fewer retries needed
const RETRY_DELAY: Duration = Duration::from_millis(2000);
/// 5ch host. Migrated from 5ch.net to 5ch.io in 2026-03 (the old net domain was revoked).
const HOST_SUFFIX: &str = "5ch.io";

/// Decodes a Shift_JIS byte sequence to UTF-8 (invalid bytes are replaced).
pub fn decode_shift_jis(bytes: &[u8]) -> String {
    let (cow, _, _) = encoding_rs::SHIFT_JIS.decode(bytes);
    cow.into_owned()
}

/// Origin to reach a given server. When `base` is empty, use the production per-server
/// 5ch.io host; otherwise route every server through the single override origin (a mock in
/// integration tests). `server`/`board`/`thread_id` are still SSRF-validated by the callers,
/// so only the host part changes — path segments remain fixed and safe.
fn origin(base: &str, server: &str) -> String {
    if base.is_empty() {
        format!("https://{server}.{HOST_SUFFIX}")
    } else {
        base.to_string()
    }
}

fn subject_url(base: &str, server: &str, board: &str) -> String {
    format!("{}/{board}/subject.txt", origin(base, server))
}
fn dat_url(base: &str, server: &str, board: &str, thread_id: &str) -> String {
    format!("{}/{board}/dat/{thread_id}.dat", origin(base, server))
}
fn setting_url(base: &str, server: &str, board: &str) -> String {
    format!("{}/{board}/SETTING.TXT", origin(base, server))
}

/// Retry wrapper shared by HEAD and GET. Retries on network errors and 5xx up to
/// `max_attempts` times; returns 2xx/4xx responses as-is (the caller interprets them).
async fn retry_request(
    build: impl Fn() -> reqwest::RequestBuilder,
    max_attempts: u32,
    label: &str,
    url: &str,
) -> Result<Response, AppError> {
    let mut last = String::new();
    for attempt in 0..max_attempts {
        match build().header("Accept-Encoding", "identity").send().await {
            Ok(resp) => {
                if resp.status().is_server_error() && attempt < max_attempts - 1 {
                    last = format!("HTTP {}", resp.status());
                    tokio::time::sleep(RETRY_DELAY).await;
                    continue;
                }
                return Ok(resp);
            }
            Err(e) => {
                last = e.to_string();
                if attempt < max_attempts - 1 {
                    tokio::time::sleep(RETRY_DELAY).await;
                }
            }
        }
    }
    Err(AppError::Upstream(format!("{label} failed: {url} ({last})")))
}

/// HEAD with retry (limited attempts; failures fall back to full GET at the call site).
async fn head(client: &Client, url: &str) -> Result<Response, AppError> {
    retry_request(|| client.head(url), HEAD_MAX_ATTEMPTS, "HEAD", url).await
}

/// GET with retry. Returns 404/2xx as-is (status handling is the caller's job).
async fn get(client: &Client, url: &str) -> Result<Response, AppError> {
    retry_request(|| client.get(url), MAX_ATTEMPTS, "GET", url).await
}

/// Fetches and parses subject.txt.
pub async fn fetch_subject(
    client: &Client,
    base: &str,
    server: &str,
    board: &str,
) -> Result<Vec<SubjectEntry>, AppError> {
    // SSRF defense-in-depth: validate before assembling the URL (thread_id is unused, so a dummy).
    validate_ref(server, board, "0")?;
    let resp = get(client, &subject_url(base, server, board)).await?;
    if !resp.status().is_success() {
        return Err(AppError::Upstream(format!("subject.txt HTTP {}", resp.status())));
    }
    let bytes = resp.bytes().await?;
    Ok(parse_subject_txt(&decode_shift_jis(&bytes)))
}

/// Fetches BBS_TITLE (the board's display name) from SETTING.TXT. Returns the board ID on failure.
pub async fn fetch_board_name(client: &Client, base: &str, server: &str, board: &str) -> String {
    // SSRF defense-in-depth: on invalid server/board, fall back to the board ID without any request.
    if validate_ref(server, board, "0").is_err() {
        return board.to_string();
    }
    let resp = match get(client, &setting_url(base, server, board)).await {
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
    /// 2xx: the whole dat body (always a full replace).
    Replace { bytes: Vec<u8> },
    /// 404: thread is gone.
    Gone,
}

/// Sends a HEAD request to the dat URL and returns the Content-Length header value.
///
/// Returns `Some(len)` on success, `None` when the header is missing or the request fails.
/// The caller must fall back to a full GET when `None` is returned.
///
/// `Accept-Encoding: identity` is required so 5ch does not compress the response and
/// returns an accurate Content-Length that matches the raw Shift-JIS byte count.
pub async fn head_dat_content_length(
    client: &Client,
    base: &str,
    server: &str,
    board: &str,
    thread_id: &str,
) -> Option<i64> {
    // SSRF defense-in-depth: validate before assembling the URL.
    validate_ref(server, board, thread_id).ok()?;
    let url = dat_url(base, server, board, thread_id);
    let resp = head(client, &url).await.ok().filter(|r| r.status().is_success())?;
    // Read Content-Length directly from the header rather than resp.content_length(),
    // because reqwest interprets HEAD response body length as 0 internally.
    resp.headers()
        .get(reqwest::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<i64>().ok())
}

/// Fetches the entire dat (no Range / no diff).
/// Returns the full body on success, Gone on 404. Take-or-skip gating is the caller's
/// responsibility (HEAD Content-Length for single-thread reload, subject.txt for board refresh).
pub async fn fetch_dat(
    client: &Client,
    base: &str,
    server: &str,
    board: &str,
    thread_id: &str,
) -> Result<DatFetch, AppError> {
    // SSRF defense-in-depth: validate before assembling the URL.
    validate_ref(server, board, thread_id)?;
    let url = dat_url(base, server, board, thread_id);
    let resp = get(client, &url).await?;
    match resp.status() {
        StatusCode::NOT_FOUND => Ok(DatFetch::Gone),
        s if s.is_success() => {
            Ok(DatFetch::Replace { bytes: resp.bytes().await?.to_vec() })
        }
        s => Err(AppError::Upstream(format!("dat HTTP {s}"))),
    }
}
