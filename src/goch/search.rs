//! find.5ch.net（公式スレタイ検索）のラップとパース。
//! ブラウザから直接叩くと CORS で弾かれるためサーバーで取得し JSON で返す。
//! レスポンスは JS 不要の HTML。結果リンクは `a.list_line_link`、href がスレ URL
//! （スキーム省略の `//server.5ch.io/test/read.cgi/board/thread_id`）、タイトルは
//! `.list_line_link_title`。タイトル末尾の `(123)` がレス数。

use crate::error::AppError;
use crate::goch::url::parse_thread_url;
use regex_lite::Regex;
use reqwest::Client;
use scraper::{Html, Selector};
use serde::Serialize;
use std::sync::LazyLock;

const SEARCH_BASE: &str = "https://find.5ch.net/search?q=";
/// find.5ch.net はブラウザ相当の UA を要求する（Monazilla だと弾かれることがある）。
const BROWSER_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/133.0.0.0 Safari/537.36";

// タイトル末尾の "(123)" からレス数を補完する。
static RES_COUNT_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\((\d+)\)\s*$").unwrap());

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SearchResult {
    pub title: String,
    pub server: String,
    pub board: String,
    pub thread_id: String,
    pub res_count: i64,
}

/// find.5ch.net で検索し、結果一覧を返す。
pub async fn search(client: &Client, query: &str) -> Result<Vec<SearchResult>, AppError> {
    let url = format!("{SEARCH_BASE}{}", urlencoding::encode(query));
    let resp = client
        .get(&url)
        .header("User-Agent", BROWSER_UA)
        .header("Accept-Encoding", "identity")
        .send()
        .await?;
    if !resp.status().is_success() {
        return Err(AppError::Upstream(format!(
            "find.5ch.net HTTP {}",
            resp.status()
        )));
    }
    let html = resp.text().await?;
    Ok(parse_search_html(&html))
}

/// find.5ch.net の検索結果 HTML をパースする。
pub fn parse_search_html(html: &str) -> Vec<SearchResult> {
    let doc = Html::parse_document(html);
    let link_sel = Selector::parse("a.list_line_link").unwrap();
    let title_sel = Selector::parse(".list_line_link_title").unwrap();

    let mut results = Vec::new();
    for link in doc.select(&link_sel) {
        let Some(href) = link.value().attr("href") else {
            continue;
        };
        // href はスキーム省略の "//server.5ch.io/..." 形式。parse_thread_url は
        // https?:// を要求するため補完する。
        let normalized = href.strip_prefix("//").map(|r| format!("https://{r}"));
        let target = normalized.as_deref().unwrap_or(href);
        let Some(tref) = parse_thread_url(target) else {
            continue;
        };

        let raw_title = link
            .select(&title_sel)
            .next()
            .map(|t| t.text().collect::<String>())
            .unwrap_or_default();
        let raw_title = raw_title.trim();

        // 末尾の "(123)" をレス数として切り出し、タイトルからは除去する。
        let (title, res_count) = match RES_COUNT_RE.captures(raw_title) {
            Some(c) => {
                let count = c[1].parse().unwrap_or(0);
                let title = raw_title[..c.get(0).unwrap().start()].trim().to_string();
                (title, count)
            }
            None => (raw_title.to_string(), 0),
        };

        results.push(SearchResult {
            title,
            server: tref.server,
            board: tref.board,
            thread_id: tref.thread_id,
            res_count,
        });
    }
    results
}

#[cfg(test)]
mod tests {
    use super::*;

    // find.5ch.net の実レスポンスから抜粋した固定 HTML サンプル。
    const SAMPLE: &str = r##"<!DOCTYPE html>
<html><body>
<div class="list">
    <div class="list_line">
        <a class="list_line_link" href="//rio2016.5ch.io/test/read.cgi/4sama/1780960049">
            <div class="list_line_link_title">【ID無し】KPOP雑談★3862【ルセラ aespa】  (588)</div>
        </a>
        <div class="list_line_info">
            <div class="list_line_info_container list_line_info_container-board"><a href="//rio2016.5ch.io/4sama/">アジアエンタメ</a></div>
            <div class="list_line_info_container">2026年06月09日 14:18</div>
        </div>
    </div>
    <div class="list_line">
        <a class="list_line_link" href="//kizuna.5ch.io/test/read.cgi/iPhone/1780556440">
            <div class="list_line_link_title">iPhone 質問スレ Part100 (仮) (23)</div>
        </a>
    </div>
</div>
</body></html>"##;

    #[test]
    fn parses_results_from_sample() {
        let results = parse_search_html(SAMPLE);
        assert_eq!(results.len(), 2);
        assert_eq!(
            results[0],
            SearchResult {
                title: "【ID無し】KPOP雑談★3862【ルセラ aespa】".into(),
                server: "rio2016".into(),
                board: "4sama".into(),
                thread_id: "1780960049".into(),
                res_count: 588,
            }
        );
    }

    #[test]
    fn keeps_parentheses_inside_title() {
        // 末尾の (23) だけがレス数。タイトル中の "(仮)" は残す。
        let results = parse_search_html(SAMPLE);
        assert_eq!(results[1].title, "iPhone 質問スレ Part100 (仮)");
        assert_eq!(results[1].res_count, 23);
        assert_eq!(results[1].server, "kizuna");
        assert_eq!(results[1].board, "iPhone");
        assert_eq!(results[1].thread_id, "1780556440");
    }

    #[test]
    fn returns_empty_for_no_results() {
        assert_eq!(parse_search_html("<html><body>no hits</body></html>"), vec![]);
    }

    #[test]
    fn skips_links_with_invalid_url() {
        let html = r#"<a class="list_line_link" href="//example.com/foo/123">
            <div class="list_line_link_title">外部リンク (10)</div></a>"#;
        assert_eq!(parse_search_html(html), vec![]);
    }

    #[test]
    fn handles_title_without_res_count() {
        let html = r#"<a class="list_line_link" href="//mi.5ch.io/test/read.cgi/news4vip/1780976160">
            <div class="list_line_link_title">レス数なしスレ</div></a>"#;
        let results = parse_search_html(html);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "レス数なしスレ");
        assert_eq!(results[0].res_count, 0);
    }
}
