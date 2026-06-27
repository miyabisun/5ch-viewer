//! Library surface of viewer-of-5ch.
//! `main.rs` is a thin wrapper over this; integration test harnesses (e.g. the
//! `itest-server` binary used by the Playwright suite) build on the same modules
//! so the very same router/DB/5ch-access code runs under test.

pub mod config;
pub mod db;
pub mod error;
pub mod fivech;
pub mod models;
pub mod routes;
pub mod sanitize;
pub mod spa;
pub mod state;
pub mod sync;
