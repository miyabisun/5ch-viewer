//! 5ch posting via bbs.cgi.
//!
//! Kept separate from http.rs (GET-only) to make the read/write boundary explicit.
//! Flow:
//!   1. POST with submit=書き込む (Shift_JIS encoded, percent-encoded body).
//!   2. If a confirmation page is returned (x-chx-error: 0000 Confirmation),
//!      extract the `feature=confirmed:<hash>` hidden field and re-POST to
//!      bbs.cgi?guid=ON with submit=上記全てを承諾して書き込む.
//!   3. Inspect x-resnum header for success; surface x-posterid / x-postdate.
//!
//! The client's cookie_store (enabled in state::build_http_client) retains
//! acorn/MonaTicket cookies so subsequent posts skip step 2 entirely.

use crate::error::AppError;
use crate::fivech::url::validate_ref;
use reqwest::{Client, Response};
use scraper::{Html, Selector};
use std::sync::LazyLock;

const HOST_SUFFIX: &str = "5ch.io";

/// Returns the base origin for the given server (or the integration-test override).
pub(crate) fn bbs_origin(base: &str, server: &str) -> String {
    if base.is_empty() {
        format!("https://{server}.{HOST_SUFFIX}")
    } else {
        base.to_string()
    }
}

/// Result of a successful post.
#[derive(Debug)]
pub struct PostResult {
    pub res_num: i64,
    pub poster_id: Option<String>,
    pub post_date: Option<String>,
}

// Selector for the `feature` hidden input (confirmation page).
static FEATURE_SEL: LazyLock<Selector> =
    LazyLock::new(|| Selector::parse("input[name='feature']").unwrap());

/// Encodes a byte slice as application/x-www-form-urlencoded percent-encoding.
/// Unlike standard urlencoding (which works on UTF-8 &str), this operates on raw
/// bytes so that Shift_JIS encoded values come through correctly.
/// Unreserved characters (RFC 3986: ALPHA / DIGIT / - / _ / . / ~) are kept as-is;
/// everything else is %-encoded.
fn percent_encode_bytes(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut out = String::with_capacity(bytes.len() * 3);
    for &b in bytes {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            // write! into a String is infallible, so unwrap is safe.
            write!(out, "%{b:02X}").unwrap();
        }
    }
    out
}

/// Encodes a UTF-8 string as Shift_JIS then percent-encodes the resulting bytes.
fn sjis_encode(s: &str) -> String {
    let (bytes, _, _) = encoding_rs::SHIFT_JIS.encode(s);
    percent_encode_bytes(&bytes)
}

/// Builds the form body for the initial POST.
/// All values are Shift_JIS + percent-encoded; keys are ASCII.
#[allow(clippy::too_many_arguments)]
fn build_form(
    board: &str,
    thread_id: &str,
    time: u64,
    from: &str,
    mail: &str,
    message: &str,
    extra: Option<(&str, &str)>, // e.g. ("feature", "confirmed:...")
    confirm: bool,               // true = use 承諾 submit label
) -> String {
    let submit_label = if confirm {
        "上記全てを承諾して書き込む"
    } else {
        "書き込む"
    };

    let mut parts = vec![
        format!("bbs={}", percent_encode_bytes(board.as_bytes())),
        format!("key={}", percent_encode_bytes(thread_id.as_bytes())),
        format!("time={time}"),
        format!("FROM={}", sjis_encode(from)),
        format!("mail={}", sjis_encode(mail)),
        format!("MESSAGE={}", sjis_encode(message)),
    ];
    if let Some((k, v)) = extra {
        parts.push(format!("{}={}", k, percent_encode_bytes(v.as_bytes())));
    }
    parts.push(format!("submit={}", sjis_encode(submit_label)));
    parts.join("&")
}

/// Extracts `feature=confirmed:...` from a confirmation-page HTML body.
/// Returns None when the input is not a confirmation page or the field is absent.
fn extract_feature(html: &str) -> Option<String> {
    let doc = Html::parse_document(html);
    let el = doc.select(&FEATURE_SEL).next()?;
    let value = el.value().attr("value")?;
    if value.starts_with("confirmed:") {
        Some(value.to_string())
    } else {
        None
    }
}

/// Reads a header as an owned string, or None when absent / non-UTF-8.
fn header_str(resp: &Response, name: &str) -> Option<String> {
    resp.headers().get(name).and_then(|v| v.to_str().ok()).map(str::to_owned)
}

/// Builds a PostResult from the response headers when x-resnum is present (= success).
/// Returns None when the post did not succeed (no resnum header).
fn post_result_from(resp: &Response) -> Option<PostResult> {
    let res_num = header_str(resp, "x-resnum")?.parse::<i64>().ok()?;
    Some(PostResult {
        res_num,
        poster_id: header_str(resp, "x-posterid"),
        post_date: header_str(resp, "x-postdate"),
    })
}

/// Determines whether the response is a confirmation page.
fn is_confirmation(x_chx_error: Option<&str>, title: &str) -> bool {
    x_chx_error.is_some_and(|v| v.contains("0000") && v.contains("Confirmation"))
        || title.contains("書き込み確認")
}

/// Extracts `<title>...</title>` from an HTML body (fast path — no full parse).
///
/// Uses a single lowercased copy for the case-insensitive position search, then
/// slices from that same lowercased string so that byte offsets always stay in sync.
/// Slicing `lower` with offsets derived from `lower` is safe; returning the lowercase
/// title text is fine here because the result is only used for error messages and
/// confirmation-page detection, not for display.
fn extract_title(html: &str) -> String {
    let lower = html.to_lowercase();
    let start = lower.find("<title>").map(|i| i + 7).unwrap_or(0);
    let end = lower[start..].find("</title>").map(|i| i + start).unwrap_or(lower.len());
    lower[start..end].trim().to_string()
}

/// Posts a message to 5ch via bbs.cgi.
///
/// SSRF-safe: validates server/board/thread_id before assembling any URL.
/// Two-stage: tries a single POST first; re-sends with the `feature` token when the
/// server returns a confirmation page (happens when acorn/MonaTicket are absent).
/// On success the cookie_store built into `client` retains the cookies automatically.
#[allow(clippy::too_many_arguments)]
pub async fn post_message(
    client: &Client,
    base: &str,
    server: &str,
    board: &str,
    thread_id: &str,
    from: &str,
    mail: &str,
    message: &str,
) -> Result<PostResult, AppError> {
    // SSRF mitigation.
    validate_ref(server, board, thread_id)?;

    let origin = bbs_origin(base, server);
    let bbs_url = format!("{origin}/test/bbs.cgi");
    let referer = format!("{origin}/test/read.cgi/{board}/{thread_id}/");

    let time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // Posts a form body to `url`, sending the shared headers.
    let send = |url: String, body: String| {
        client
            .post(url)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .header("Referer", &referer)
            .header("Accept-Encoding", "identity")
            .body(body)
            .send()
    };

    // --- First attempt ---
    let body1 = build_form(board, thread_id, time, from, mail, message, None, false);
    let resp1 = send(bbs_url.clone(), body1).await?;

    // Success on first attempt (cookie was valid, no confirmation needed).
    if let Some(result) = post_result_from(&resp1) {
        return Ok(result);
    }

    // Read the body to determine whether this is a confirmation page or an error.
    let chx1 = header_str(&resp1, "x-chx-error");
    let bytes1 = resp1.bytes().await.map_err(|e| AppError::Upstream(e.to_string()))?;
    let (html1, _, _) = encoding_rs::SHIFT_JIS.decode(&bytes1);
    let title1 = extract_title(&html1);

    if !is_confirmation(chx1.as_deref(), &title1) {
        return Err(AppError::Upstream(format!("bbs.cgi: {title1}")));
    }

    // --- Confirmation page: extract feature and re-send ---
    let feature = extract_feature(&html1)
        .ok_or_else(|| AppError::Upstream("confirmation page: feature field not found".into()))?;

    let body2 = build_form(
        board,
        thread_id,
        time,
        from,
        mail,
        message,
        Some(("feature", &feature)),
        true,
    );
    let resp2 = send(format!("{bbs_url}?guid=ON"), body2).await?;

    if let Some(result) = post_result_from(&resp2) {
        return Ok(result);
    }

    // Neither attempt succeeded — read the second response body for a diagnostic message.
    let bytes2 = resp2.bytes().await.map_err(|e| AppError::Upstream(e.to_string()))?;
    let (html2, _, _) = encoding_rs::SHIFT_JIS.decode(&bytes2);
    let title2 = extract_title(&html2);
    Err(AppError::Upstream(format!("bbs.cgi (after confirmation): {title2}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- percent_encode_bytes ---

    #[test]
    fn percent_encode_ascii_alphanumeric_passthrough() {
        let out = percent_encode_bytes(b"abc123");
        assert_eq!(out, "abc123");
    }

    #[test]
    fn percent_encode_unreserved_chars_passthrough() {
        // RFC 3986 unreserved: - _ . ~
        let out = percent_encode_bytes(b"-_.~");
        assert_eq!(out, "-_.~");
    }

    #[test]
    fn percent_encode_space_is_percent_20() {
        assert_eq!(percent_encode_bytes(b" "), "%20");
    }

    // --- sjis_encode ---

    /// "あ" in Shift_JIS is 0x82 0xA0.
    #[test]
    fn sjis_encode_hiragana_a() {
        let out = sjis_encode("あ");
        assert_eq!(out, "%82%A0");
    }

    /// "書き込む" encodes to known SJIS bytes.
    #[test]
    fn sjis_encode_submit_label() {
        let out = sjis_encode("書き込む");
        // SJIS: 書=0x8F91 き=0x82AB 込=0x8D9E む=0x82DE
        assert_eq!(out, "%8F%91%82%AB%8D%9E%82%DE");
    }

    /// "上記全てを承諾して書き込む" is the confirmation submit label.
    #[test]
    fn sjis_encode_confirmation_submit() {
        let out = sjis_encode("上記全てを承諾して書き込む");
        // Must be non-empty and start with a % (all kanji are outside ASCII range).
        assert!(out.starts_with('%'), "confirmation submit must be percent-encoded: {out}");
        // Must not contain any raw multi-byte character — all bytes outside ASCII must be encoded.
        assert!(
            !out.chars().any(|c| c as u32 > 0x7f),
            "output must contain only ASCII percent-encoded chars: {out}"
        );
    }

    // --- extract_feature ---

    #[test]
    fn extract_feature_finds_confirmed_hash() {
        let html = r#"<html><body>
            <form action="bbs.cgi?guid=ON" method="post">
              <input type="hidden" name="bbs" value="software" />
              <input type="hidden" name="key" value="1779846933" />
              <input type="hidden" name="feature" value="confirmed:a1b2c3d4e5f60000111122223333444455556666" />
              <input type="submit" name="submit" value="上記全てを承諾して書き込む" />
            </form>
        </body></html>"#;
        let result = extract_feature(html);
        assert_eq!(
            result,
            Some("confirmed:a1b2c3d4e5f60000111122223333444455556666".to_string())
        );
    }

    #[test]
    fn extract_feature_returns_none_when_absent() {
        let html = r#"<html><body><p>No form here</p></body></html>"#;
        assert_eq!(extract_feature(html), None);
    }

    #[test]
    fn extract_feature_returns_none_for_non_confirmed_value() {
        let html = r#"<html><body>
            <input type="hidden" name="feature" value="other:value" />
        </body></html>"#;
        // Value does not start with "confirmed:" so it must be rejected.
        assert_eq!(extract_feature(html), None);
    }

    // --- extract_title ---

    #[test]
    fn extract_title_basic() {
        let html = "<html><head><title>書きこみました。</title></head><body></body></html>";
        // Returns lowercase (safe for error-message / detection use).
        assert_eq!(extract_title(html), "書きこみました。");
    }

    #[test]
    fn extract_title_case_insensitive() {
        // TITLE in uppercase: must still be found.
        let html = "<TITLE>  書き込み確認  </TITLE>";
        assert!(extract_title(html).contains("書き込み確認"));
    }

    /// Regression: previously `to_lowercase()` offsets were used to slice the original
    /// `html` string.  When characters before `<title>` expand on lowercasing (e.g. the
    /// Turkish dotted I 'İ' → 'i\u{307}', 2 bytes → 3 bytes), the offset into `html`
    /// would be wrong and produce garbage.  Now we slice `lower` with offsets from
    /// `lower`, so the result is always consistent.
    #[test]
    fn extract_title_offset_safe_with_expanding_lowercase_char() {
        // U+0130 'İ' lowercases to 'i' + combining dot (2 bytes → 3 bytes in UTF-8),
        // which shifts all subsequent byte offsets in the lowercased string relative to
        // the original.  The fix: slice `lower[start..end]` not `html[start..end]`.
        let html = "İ<title>書きこみました。</title>";
        let title = extract_title(html);
        // Must contain the success phrase, not garbage bytes.
        assert!(
            title.contains("書きこみました"),
            "extract_title must not produce garbled text: got {title:?}"
        );
    }

    // --- is_confirmation ---

    #[test]
    fn is_confirmation_detects_x_chx_error_header() {
        assert!(is_confirmation(Some("0000 Confirmation"), "some title"));
    }

    #[test]
    fn is_confirmation_detects_title_fallback() {
        assert!(is_confirmation(None, "■ 書き込み確認 ■"));
    }

    #[test]
    fn is_confirmation_false_for_success_page() {
        assert!(!is_confirmation(None, "書きこみました。"));
        assert!(!is_confirmation(Some("1234 Error"), "書きこみました。"));
    }

    // --- build_form ---

    #[test]
    fn build_form_contains_all_required_fields() {
        let form = build_form("software", "1779846933", 1000, "", "sage", "テスト", None, false);
        assert!(form.contains("bbs=software"), "bbs field missing: {form}");
        assert!(form.contains("key=1779846933"), "key field missing: {form}");
        assert!(form.contains("time=1000"), "time field missing: {form}");
        assert!(form.contains("mail="), "mail field missing: {form}");
        // MESSAGE encodes "テスト" in SJIS (%83e=%83X=%83g in SJIS).
        assert!(form.contains("MESSAGE="), "MESSAGE field missing: {form}");
        assert!(form.contains("submit="), "submit field missing: {form}");
    }

    #[test]
    fn build_form_confirm_uses_alternate_submit() {
        let form_normal = build_form("b", "1", 0, "", "", "msg", None, false);
        let form_confirm = build_form("b", "1", 0, "", "", "msg", None, true);
        // Normal uses 書き込む, confirm uses 上記全てを承諾して書き込む (both SJIS-encoded).
        assert_ne!(form_normal, form_confirm, "confirm forms must differ");
    }

    #[test]
    fn build_form_extra_feature_appears_in_body() {
        let form = build_form(
            "b",
            "1",
            0,
            "",
            "",
            "msg",
            Some(("feature", "confirmed:abc")),
            true,
        );
        assert!(form.contains("feature=confirmed%3Aabc") || form.contains("feature=confirmed:abc"),
            "feature field must appear in form: {form}");
    }
}
