//! The real app router on an in-memory database, with a stand-in for 5ch beside it:
//! `src/bin/itest-server.rs`, started for a test on ports the OS chooses. Its image cache and
//! cookies lie in a temporary directory of its own, and it serves the page from `client/build`.
use serde_json::Value;
use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    time::Duration,
};

/// One itest-server, ended when it is dropped.
pub struct Itest {
    child: Child,
    /// The app, as `http://127.0.0.1:<port>`.
    pub app: String,
    /// The stand-in for 5ch, as `http://127.0.0.1:<port>`.
    pub mock: String,
    http: reqwest::Client,
    _dir: tempfile::TempDir,
}

impl Itest {
    /// Starts one, and returns once both of its servers listen.
    pub fn start() -> Self {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let images = dir.path().join("images");
        let mut child = Command::new(env!("CARGO_BIN_EXE_itest-server"))
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .env("IMAGE_CACHE_DIR", &images)
            .env("COOKIES_PATH", dir.path().join("cookies.json"))
            // loopback image downloads (the stand-in's images) are allowed in tests only
            .env("FIVECH_ALLOW_LOOPBACK_FOR_TEST", "1")
            .env("RUST_LOG", "warn")
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("itest-server starts");
        let mut line = String::new();
        BufReader::new(child.stdout.take().expect("its output"))
            .read_line(&mut line)
            .expect("itest-server says where it listens");
        let address = |name: &str| {
            line.split_whitespace()
                .find_map(|part| part.strip_prefix(&format!("{name}=")))
                .unwrap_or_else(|| panic!("itest-server did not start: {line:?}"))
                .to_owned()
        };
        Self {
            app: address("app"),
            mock: address("mock"),
            child,
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .expect("an HTTP client"),
            _dir: dir,
        }
    }

    /// Sends `method` to `url` (with `body` as JSON unless it is null), and answers with the
    /// status and the body, read as JSON (null when it is not JSON).
    pub async fn send(&self, method: reqwest::Method, url: &str, body: &Value) -> (u16, Value) {
        let (status, _, body) = self.answer(method, url, body).await;
        (status, body)
    }

    /// As [`Itest::send`], with the headers of the answer.
    pub async fn answer(
        &self,
        method: reqwest::Method,
        url: &str,
        body: &Value,
    ) -> (u16, reqwest::header::HeaderMap, Value) {
        let mut request = self.http.request(method, url);
        if !body.is_null() {
            request = request
                .header("content-type", "application/json")
                .body(body.to_string());
        }
        let answer = request
            .send()
            .await
            .unwrap_or_else(|error| panic!("{url}: {error}"));
        let status = answer.status().as_u16();
        let headers = answer.headers().clone();
        let bytes = answer.bytes().await.unwrap_or_default();
        (
            status,
            headers,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    /// GET of `url`, which must succeed: its JSON.
    pub async fn get(&self, url: &str) -> Value {
        let (status, body) = self.send(reqwest::Method::GET, url, &Value::Null).await;
        assert!((200..300).contains(&status), "GET {url}: {status} {body}");
        body
    }

    /// POST of `body` to `url`, which must succeed: its JSON.
    pub async fn post(&self, url: &str, body: Value) -> Value {
        let (status, answer) = self.send(reqwest::Method::POST, url, &body).await;
        assert!(
            (200..300).contains(&status),
            "POST {url}: {status} {answer}"
        );
        answer
    }
}

impl Drop for Itest {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
