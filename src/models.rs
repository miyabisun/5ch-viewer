use crate::goch::dat::Res;
use serde::{Deserialize, Serialize};

/// お気に入りスレッド（一覧 API レスポンス）。
/// 並べ替えはフロント責務のため ORDER BY せず順不同で返す。
#[derive(Debug, Serialize)]
pub struct Favorite {
    pub server: String,
    pub board: String,
    pub board_name: String,
    pub thread_id: String,
    pub title: String,
    pub res_count: i64,
    pub read_res: i64,
    pub rating: i64,
    pub status: String,
}

/// お気に入り追加リクエスト。url 直接 か server/board/thread_id のどちらか。
#[derive(Debug, Deserialize)]
pub struct AddRequest {
    pub url: Option<String>,
    pub server: Option<String>,
    pub board: Option<String>,
    pub thread_id: Option<String>,
    pub title: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ProgressRequest {
    pub read_res: i64,
}

#[derive(Debug, Deserialize)]
pub struct RatingRequest {
    pub rating: i64,
}

/// スレ本文（保存済み dat）レスポンス。
#[derive(Debug, Serialize)]
pub struct DatResponse {
    pub title: String,
    pub res_count: i64,
    pub read_res: i64,
    pub status: String,
    pub res: Vec<Res>,
}

/// リロード（Range 差分取得）結果。
#[derive(Debug, Serialize)]
pub struct ReloadResponse {
    pub res_count: i64,
    pub read_res: i64,
    pub status: String,
    /// dat に変化があったか（NotModified なら false）。
    pub updated: bool,
}
