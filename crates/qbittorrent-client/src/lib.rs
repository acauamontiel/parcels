//! Async client for the qBittorrent WebUI API.
//!
//! This crate has no GTK dependency. It needs a Tokio runtime, because it is
//! built on `reqwest`.
//!
//! Supported servers: qBittorrent 5.0 and later. API key authentication
//! requires qBittorrent 5.2 (WebAPI 2.14.1) or later.

mod client;
mod error;
pub mod maindata;
mod state;
mod types;
mod version;

pub use client::{Client, ClientConfig, Credentials, normalize_url};
pub use error::{Error, Result};
pub use maindata::MainData;
pub use state::TorrentState;
pub use types::ServerInfo;
pub use version::Version;

/// Oldest qBittorrent release Parcels talks to: 5.0 renamed the pause/resume
/// endpoints to stop/start and the `paused` parameter to `stopped`.
pub const MIN_SERVER_VERSION: Version = Version::new(5, 0, 0);
