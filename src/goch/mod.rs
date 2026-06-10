//! 5ch (goch) access layer.
//! Starting with pure functions that use neither HTTP nor DB (parsing, next-thread detection).

pub mod dat;
pub mod http;
pub mod next_thread;
pub mod refresh;
pub mod search;
pub mod subject;
pub mod url;
