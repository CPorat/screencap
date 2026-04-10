//! HTTP API module
//!
//! Axum HTTP server on localhost:7878

pub mod routes;
pub mod server;

pub use server::serve;
