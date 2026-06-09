//! 5ch への HTTP アクセス。
//! - User-Agent は client のデフォルト（Monazilla 入り。state::USER_AGENT）。
//! - 圧縮で Content-Length / Range が壊れないよう常に `Accept-Encoding: identity`。
//! - 5xx はリトライ、404 はリトライしない。

use crate::error::AppError;
use crate::goch::dat::validate_diff;
use crate::goch::subject::{parse_subject_txt, SubjectEntry};
use reqwest::{Client, Response, StatusCode};
use std::time::Duration;

const MAX_ATTEMPTS: u32 = 4;
const RETRY_DELAY: Duration = Duration::from_millis(2000);
/// 境界照合に使う末尾バイト数（Shift_JIS 2バイト×3文字相当）。
/// dat 末尾は必ず改行(0x0a)終端なので 1 バイトでは衝突する。実機確認済み。
const OVERLAP: u64 = 6;
/// 5ch のホスト。2026-03 に 5ch.net → 5ch.io へ移転（旧 net ドメインは剥奪済み）。
const HOST_SUFFIX: &str = "5ch.io";

/// Shift_JIS バイト列を UTF-8 にデコード（不正バイトは置換）。
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

/// GET（identity 固定 + 任意の Range）。ネットワークエラーと 5xx はリトライ。
/// 404/416/206/2xx はリトライせずそのまま返す（ステータス判定は呼び出し側）。
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

/// subject.txt を取得してパース。
pub async fn fetch_subject(
    client: &Client,
    server: &str,
    board: &str,
) -> Result<Vec<SubjectEntry>, AppError> {
    let resp = get(client, &subject_url(server, board), None).await?;
    if !resp.status().is_success() {
        return Err(AppError::Upstream(format!("subject.txt HTTP {}", resp.status())));
    }
    let bytes = resp.bytes().await?;
    Ok(parse_subject_txt(&decode_shift_jis(&bytes)))
}

/// SETTING.TXT の BBS_TITLE（板の日本語名）を取得。失敗時は board ID を返す。
pub async fn fetch_board_name(client: &Client, server: &str, board: &str) -> String {
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

/// dat の Range 差分取得結果。
#[derive(Debug)]
pub enum DatFetch {
    /// 206: 増分（末尾に追記すべきバイト列）と新しい総バイト数。
    Append { bytes: Vec<u8>, total: u64 },
    /// 200 もしくは縮小検知後の再取得: 全体を置換。
    Replace { bytes: Vec<u8>, total: u64 },
    /// 416 かつ サイズ同一: 変化なし。
    NotModified,
    /// 404: スレ落ち。
    Gone,
}

/// dat を Range 差分取得する。
/// `from_bytes` は前回保存した Shift_JIS バイト数、`last_tail` は前回 dat の末尾
/// 最大 OVERLAP バイト（境界照合用）。あぼーん等の壊れた差分は全取得でリペアする。
pub async fn fetch_dat(
    client: &Client,
    server: &str,
    board: &str,
    thread_id: &str,
    from_bytes: u64,
    last_tail: &[u8],
) -> Result<DatFetch, AppError> {
    let url = dat_url(server, board, thread_id);

    // 初回は全取得。
    if from_bytes == 0 {
        return refetch_full(client, &url).await;
    }

    // 末尾 overlap バイトを含めて取得し、境界一致と差分の妥当性を確認する。
    let overlap = OVERLAP.min(from_bytes).min(last_tail.len() as u64);
    let resp = get(client, &url, Some(from_bytes - overlap)).await?;

    match resp.status() {
        // サーバーが Range を無視して全体を返した。
        StatusCode::OK => {
            let bytes = resp.bytes().await?.to_vec();
            let total = bytes.len() as u64;
            Ok(DatFetch::Replace { bytes, total })
        }
        StatusCode::PARTIAL_CONTENT => {
            let bytes = resp.bytes().await?.to_vec();
            let o = overlap as usize;
            // ① 境界一致: 先頭 overlap バイトが前回末尾と一致するか（あぼーん検知）。
            let boundary_ok =
                o == 0 || (bytes.len() >= o && bytes[..o] == last_tail[last_tail.len() - o..]);
            let appended = if bytes.len() >= o {
                bytes[o..].to_vec()
            } else {
                Vec::new()
            };
            // ② 差分のヘッダー妥当性（5 バイト問題等の壊れた追記を弾く）。
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
            // ④ サーバー側サイズ <= 要求開始位置。Content-Range の total で判定。
            match content_range_total(&resp) {
                Some(t) if t == from_bytes => Ok(DatFetch::NotModified),
                _ => refetch_full(client, &url).await, // 縮小(あぼーん)/不明 → 全取得
            }
        }
        StatusCode::NOT_FOUND => Ok(DatFetch::Gone),
        s => Err(AppError::Upstream(format!("dat HTTP {s}"))),
    }
}

/// dat 全体を取り直す（差分が壊れていた場合のリペア・初回取得）。
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

/// Content-Range ヘッダから総サイズを取り出す（"bytes */1234" や "bytes 0-1/1234"）。
fn content_range_total(resp: &Response) -> Option<u64> {
    let v = resp.headers().get(reqwest::header::CONTENT_RANGE)?;
    v.to_str().ok()?.rsplit('/').next()?.trim().parse().ok()
}
