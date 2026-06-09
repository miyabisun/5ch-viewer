//! 5ch（goch）アクセス層。
//! まずは HTTP も DB も使わない純粋関数（パース・次スレ判定）から。

pub mod dat;
pub mod http;
pub mod next_thread;
pub mod search;
pub mod subject;
pub mod url;
