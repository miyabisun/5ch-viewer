//! Wrapping and parsing of ff5ch.syoboi.jp (third-party thread-title search by syoboi.jp).
//! The official 5ch search has poor precision (unrelated threads rank high); ff5ch gives
//! far better results (exact substring match, newest-first). The server fetches it to
//! avoid CORS; response is RSS 2.0 XML.

use crate::error::AppError;
use crate::fivech::url::parse_thread_url;
use crate::state::USER_AGENT;
use quick_xml::escape::unescape;
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use regex_lite::Regex;
use reqwest::Client;
use serde::Serialize;
use std::sync::LazyLock;

const SEARCH_BASE: &str = "https://ff5ch.syoboi.jp/?q=";

// Derive the post count from the trailing "(123)" in the title.
static RES_COUNT_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\((\d+)\)\s*$").unwrap());

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SearchResult {
    pub title: String,
    pub server: String,
    pub board: String,
    pub thread_id: String,
    pub res_count: i64,
}

/// Searches via ff5ch.syoboi.jp and returns the result list.
pub async fn search(client: &Client, query: &str) -> Result<Vec<SearchResult>, AppError> {
    let url = format!("{SEARCH_BASE}{}&alt=rss", urlencoding::encode(query));
    let resp = client
        .get(&url)
        .header("User-Agent", USER_AGENT)
        .send()
        .await?;
    if !resp.status().is_success() {
        return Err(AppError::Upstream(format!(
            "ff5ch.syoboi.jp HTTP {}",
            resp.status()
        )));
    }
    Ok(parse_search_rss(&resp.text().await?))
}

/// Parses the RSS 2.0 search result from ff5ch.syoboi.jp.
///
/// Each `<item>` provides `<title>` (with trailing `  (N)` as post count) and
/// `<guid>` (absolute thread URL). The `<channel>`-level `<title>` is skipped
/// because we only capture children of `<item>`.
pub fn parse_search_rss(xml: &str) -> Vec<SearchResult> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    reader.config_mut().expand_empty_elements = true;

    let mut results = Vec::new();
    let mut in_item = false;
    let mut title = String::new();
    let mut guid = String::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => match e.name().as_ref() {
                b"item" => {
                    in_item = true;
                    title.clear();
                    guid.clear();
                }
                // read_text consumes the element body and returns its text content;
                // we then unescape XML entities (&amp; etc).
                b"title" if in_item => title = read_unescaped(&mut reader, e.to_end().name()),
                b"guid" if in_item => guid = read_unescaped(&mut reader, e.to_end().name()),
                _ => {}
            },
            Ok(Event::End(e)) if e.name().as_ref() == b"item" => {
                in_item = false;
                if let Some(r) = build_result(&title, &guid) {
                    results.push(r);
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    results
}

/// Reads the inner text of the current element up to `end` and XML-unescapes it.
/// Returns an empty string if the read or unescape fails.
fn read_unescaped(reader: &mut Reader<&[u8]>, end: quick_xml::name::QName<'_>) -> String {
    reader
        .read_text(end)
        .ok()
        .and_then(|raw| unescape(&raw).ok().map(|c| c.into_owned()))
        .unwrap_or_default()
}

/// Splits trailing "  (N)" off the title and pairs it with a parsed thread URL.
/// Returns None when the guid is not a valid 5ch thread URL.
fn build_result(raw_title: &str, guid: &str) -> Option<SearchResult> {
    // guid may start with http:// — parse_thread_url accepts https?://.
    let tref = parse_thread_url(guid.trim())?;
    let raw = raw_title.trim();
    let (title, res_count) = match RES_COUNT_RE.captures(raw) {
        Some(c) => (
            raw[..c.get(0).unwrap().start()].trim().to_string(),
            c[1].parse().unwrap_or(0),
        ),
        None => (raw.to_string(), 0),
    };
    Some(SearchResult {
        title,
        server: tref.server,
        board: tref.board,
        thread_id: tref.thread_id,
        res_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // Fixed RSS sample excerpted from a real ff5ch.syoboi.jp response.
    // Item 1 uses http:// guid with trailing slash to verify both are accepted.
    const SAMPLE: &str = r##"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0" xmlns:dc="http://purl.org/dc/elements/1.1/">
  <channel>
    <title>5ch検索 「test」</title>
    <item>
      <title>FDM式3Dプリンター個人向け 37レイヤー目  (850)</title>
      <guid ispermalink="true">http://medaka.5ch.io/test/read.cgi/printer/1779746829/</guid>
      <description>FDM式3Dプリンター個人向け 37レイヤー目 </description>
      <pubdate>Tue, 26 May 2026 07:07:09 +0900</pubdate>
      <category>プリンタ</category>
    </item>
    <item>
      <title>iPhone 質問スレ Part100 (仮)  (23)</title>
      <guid ispermalink="true">http://kizuna.5ch.io/test/read.cgi/iPhone/1780556440/</guid>
      <description>iPhone 質問スレ</description>
      <pubdate>Mon, 09 Jun 2026 10:00:00 +0900</pubdate>
      <category>iPhone</category>
    </item>
  </channel>
</rss>"##;

    #[test]
    fn parses_results_from_sample() {
        let results = parse_search_rss(SAMPLE);
        assert_eq!(results.len(), 2);
        // Item 1: http:// guid with trailing slash must parse correctly.
        assert_eq!(
            results[0],
            SearchResult {
                title: "FDM式3Dプリンター個人向け 37レイヤー目".into(),
                server: "medaka".into(),
                board: "printer".into(),
                thread_id: "1779746829".into(),
                res_count: 850,
            }
        );
    }

    #[test]
    fn keeps_parentheses_inside_title() {
        // Only the trailing (23) is the post count. The "(仮)" within the title is kept.
        let results = parse_search_rss(SAMPLE);
        assert_eq!(results[1].title, "iPhone 質問スレ Part100 (仮)");
        assert_eq!(results[1].res_count, 23);
        assert_eq!(results[1].server, "kizuna");
        assert_eq!(results[1].board, "iPhone");
        assert_eq!(results[1].thread_id, "1780556440");
    }

    #[test]
    fn returns_empty_for_no_results() {
        let xml = r##"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0"><channel><title>5ch検索 「nothing」</title></channel></rss>"##;
        assert_eq!(parse_search_rss(xml), vec![]);
    }

    #[test]
    fn skips_links_with_invalid_url() {
        let xml = r##"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0"><channel>
  <item>
    <title>外部リンク (10)</title>
    <guid ispermalink="true">http://example.com/foo/123</guid>
  </item>
</channel></rss>"##;
        assert_eq!(parse_search_rss(xml), vec![]);
    }

    #[test]
    fn handles_title_without_res_count() {
        let xml = r##"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0"><channel>
  <item>
    <title>レス数なしスレ</title>
    <guid ispermalink="true">http://mi.5ch.io/test/read.cgi/news4vip/1780976160/</guid>
  </item>
</channel></rss>"##;
        let results = parse_search_rss(xml);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "レス数なしスレ");
        assert_eq!(results[0].res_count, 0);
    }

    #[test]
    fn ignores_channel_level_title() {
        // <channel><title> must not appear as a search result.
        let xml = r##"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0"><channel>
  <title>5ch検索 「xxx」</title>
  <item>
    <title>正しいスレタイ (42)</title>
    <guid ispermalink="true">http://eagle.5ch.io/test/read.cgi/livejupiter/1700000000/</guid>
  </item>
</channel></rss>"##;
        let results = parse_search_rss(xml);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "正しいスレタイ");
        assert_eq!(results[0].res_count, 42);
    }
}
